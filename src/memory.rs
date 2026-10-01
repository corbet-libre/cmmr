use crate::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
};

#[derive(Default)]
struct Data {
    records: BTreeMap<String, Record>,
    indexes: BTreeMap<String, BTreeSet<String>>,
    generations: BTreeMap<String, BTreeSet<String>>,
    expiries: BTreeSet<String>,
    last: u64,
    closed: bool,
}
impl Data {
    fn remove(&mut self, key: &str) {
        if let Some(record) = self.records.remove(key) {
            let position = record.position();
            self.expiries.remove(&position);
            for index in &record.indexes {
                // All three secondary maps are written with the record while
                // holding this mutex. A committed record always owns its rows.
                let entries = self.indexes.get_mut(index).expect("committed index row");
                entries.remove(&position);
                if entries.is_empty() {
                    self.indexes.remove(index);
                }
            }
            let entries = self
                .generations
                .get_mut(&record.generation)
                .expect("committed generation row");
            entries.remove(key);
            if entries.is_empty() {
                self.generations.remove(&record.generation);
            }
        }
    }
    fn observe(&mut self, now: u64) -> Result<(), Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        if now > MAX_TIME {
            return Err(Error::Invalid);
        }
        if now < self.last {
            return Err(Error::ClockRegression);
        }
        self.last = now;
        Ok(())
    }
    fn prune(&mut self, now: u64) -> usize {
        let mut count = 0;
        while let Some(position) = self.expiries.first() {
            let key = position[17..].to_owned();
            if self.records[&key].deadline.unix_ms() > now {
                break;
            }
            self.remove(&key);
            count += 1;
        }
        count
    }
}

/// One bounded scope, shared by reference. Dropping it loses all state.
pub struct Memory<C> {
    clock: C,
    scope: Scope,
    limits: Limits,
    data: Mutex<Data>,
}
impl<C: Clock> Memory<C> {
    pub fn new(scope: Scope, limits: Limits, clock: C) -> Result<Self, Error> {
        Scope::new(scope.community(), scope.incarnation())?;
        Ok(Self {
            scope,
            limits: limits.validate()?,
            clock,
            data: Mutex::new(Data::default()),
        })
    }
    fn operate<T>(&self, f: impl FnOnce(&mut Data, u64) -> Result<T, Error>) -> Result<T, Error> {
        let mut data = self.data.lock().map_err(|_| Error::Unavailable)?;
        let now = self.clock.now()?;
        data.observe(now)?;
        f(&mut data, now)
    }
}
impl<C> fmt::Debug for Memory<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Memory([REDACTED])")
    }
}
impl<C: Clock> Store for Memory<C> {
    fn state(&self) -> State {
        match self.data.lock() {
            Ok(data) if data.closed => State::Closed,
            Ok(_) => State::Ready,
            Err(_) => State::Unavailable,
        }
    }
    async fn get(&self, key: &str) -> Result<Option<Record>, Error> {
        validate_name(key)?;
        self.operate(|data, now| {
            if data
                .records
                .get(key)
                .is_some_and(|r| r.deadline.unix_ms() <= now)
            {
                data.remove(key);
            }
            Ok(data.records.get(key).cloned())
        })
    }
    async fn compare_exchange(&self, batch: &[Write]) -> Result<(), Error> {
        self.operate(|data, now| {
            validate_batch(batch, self.limits, now)?;
            data.prune(now);
            let mut size = data.records.len();
            for write in batch {
                let current = data.records.get(&write.record.key);
                if current.map(|r| r.revision) != write.expected {
                    return Err(Error::Conflict);
                }
                if current.is_none() {
                    size += 1;
                }
            }
            if size > self.limits.records {
                return Err(Error::Capacity);
            }
            for write in batch {
                let record = write.record.clone();
                data.remove(&record.key);
                let position = record.position();
                data.expiries.insert(position.clone());
                data.generations
                    .entry(record.generation.clone())
                    .or_default()
                    .insert(record.key.clone());
                for index in &record.indexes {
                    data.indexes
                        .entry(index.clone())
                        .or_default()
                        .insert(position.clone());
                }
                data.records.insert(record.key.clone(), record);
            }
            Ok(())
        })
    }
    async fn delete_generation(&self, generation: &str) -> Result<usize, Error> {
        validate_name(generation)?;
        self.operate(|data, now| {
            data.prune(now);
            let keys = data
                .generations
                .get(generation)
                .cloned()
                .unwrap_or_default();
            for key in &keys {
                data.remove(key);
            }
            Ok(keys.len())
        })
    }
    async fn page(
        &self,
        index: &str,
        cursor: Option<&Cursor>,
        limit: usize,
    ) -> Result<Page, Error> {
        validate_name(index)?;
        if limit == 0 || limit > self.limits.page {
            return Err(Error::Invalid);
        }
        self.operate(|data, now| {
            let deadline = if let Some(cursor) = cursor {
                cursor.check(&self.scope, index, now, self.limits)?;
                cursor.deadline()
            } else {
                // observe bounds now by MAX_TIME; validated TTL is at most one
                // year. Their sum fits u64, and Deadline enforces MAX_TIME.
                Deadline::new(now + self.limits.ttl_ms)?
            };
            data.prune(now);
            let start = cursor.map_or(String::new(), |c| c.after().to_owned());
            let records: Vec<Record> = data
                .indexes
                .get(index)
                .into_iter()
                .flat_map(|set| {
                    set.range((
                        std::ops::Bound::Excluded(start.clone()),
                        std::ops::Bound::Unbounded,
                    ))
                })
                .take(limit)
                .map(|position| data.records[&position[17..]].clone())
                .collect();
            let next = if records.len() == limit {
                records.last().map(|r| {
                    Cursor::new(self.scope.clone(), index.to_owned(), r.position(), deadline)
                })
            } else {
                None
            };
            Ok(Page { records, next })
        })
    }
    async fn maintain(&self) -> Result<usize, Error> {
        self.operate(|data, now| Ok(data.prune(now)))
    }
    fn close(&self) {
        // Poisoning fences use, but must not prevent explicit erasure.
        let mut data = self
            .data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *data = Data {
            closed: true,
            ..Data::default()
        };
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "../tests/internal/poison.rs"]
mod tests;
