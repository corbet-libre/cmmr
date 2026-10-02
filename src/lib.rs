//! Bounded, expiring storage. The port and conformance vectors live with this
//! implementation so adapters share one contract without a protocol crate.
#![forbid(unsafe_code)]

mod memory;
pub use memory::Memory;
#[cfg(feature = "conformance")]
pub mod conformance;

use serde::{Deserialize, Serialize};
use std::{fmt, future::Future, sync::Arc};

/// Largest exact integer shared with the backend's Lua number representation.
pub const MAX_TIME: u64 = (1 << 53) - 1;

/// Caller-supplied Unix milliseconds. No hidden wall clock is installed.
pub trait Clock: Send + Sync {
    fn now(&self) -> Result<u64, Error>;
}
impl<T: Clock> Clock for Arc<T> {
    fn now(&self) -> Result<u64, Error> {
        self.as_ref().now()
    }
}

/// Stable error classes never contain input or dependency details.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Expired,
    Conflict,
    Capacity,
    ClockRegression,
    Unavailable,
    Closed,
    Corrupt,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "volatile storage: {self:?}")
    }
}
impl std::error::Error for Error {}

/// Caller-selected caps, enforced identically by both stores, per scoped store.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub records: usize,
    pub value_bytes: usize,
    pub batch: usize,
    pub indexes_per_record: usize,
    pub page: usize,
    pub ttl_ms: u64,
}
impl Limits {
    pub fn validate(self) -> Result<Self, Error> {
        if self.records == 0
            || self.records > 1_000_000
            || self.value_bytes == 0
            || self.value_bytes > 1_048_576
            || self.batch == 0
            || self.batch > 256
            || self.indexes_per_record > 32
            || self.page == 0
            || self.page > 1024
            || self.ttl_ms == 0
            || self.ttl_ms > 31_536_000_000
        {
            return Err(Error::Invalid);
        }
        Ok(self)
    }
}

/// Opaque, service-authorized community and fresh service incarnation.
/// All processes sharing a running service use the same incarnation. A restart
/// uses a fresh one; retaining it would deliberately reopen still-live data.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    community: String,
    incarnation: String,
}
impl Scope {
    pub fn new(
        community: impl Into<String>,
        incarnation: impl Into<String>,
    ) -> Result<Self, Error> {
        let value = Self {
            community: community.into(),
            incarnation: incarnation.into(),
        };
        validate_name(&value.community)?;
        validate_name(&value.incarnation)?;
        Ok(value)
    }
    pub fn community(&self) -> &str {
        &self.community
    }
    pub fn incarnation(&self) -> &str {
        &self.incarnation
    }
}
impl fmt::Debug for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Scope([REDACTED])")
    }
}

/// Caller-generated unique commit token. Never reuse across creations or writes.
/// Matching this token in current records reconciles an uncertain commit.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision(pub [u8; 16]);
impl fmt::Debug for Revision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Revision([REDACTED])")
    }
}

/// Absolute exclusive deadline, required in every write.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct Deadline(u64);
impl Deadline {
    pub fn new(unix_ms: u64) -> Result<Self, Error> {
        if unix_ms == 0 || unix_ms > MAX_TIME {
            return Err(Error::Invalid);
        }
        Ok(Self(unix_ms))
    }
    pub fn unix_ms(self) -> u64 {
        self.0
    }
    pub fn check(self, now: u64, limits: Limits) -> Result<(), Error> {
        if self.0 <= now {
            return Err(Error::Expired);
        }
        if self.0 - now > limits.ttl_ms {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
impl TryFrom<u64> for Deadline {
    type Error = Error;
    fn try_from(value: u64) -> Result<Self, Error> {
        Self::new(value)
    }
}
impl From<Deadline> for u64 {
    fn from(value: Deadline) -> Self {
        value.0
    }
}

/// Complete replacement, including all index memberships. No free-standing
/// index writes exist, so a committed index can only refer to its owning record.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub key: String,
    pub generation: String,
    pub revision: Revision,
    pub deadline: Deadline,
    pub value: Vec<u8>,
    pub indexes: Vec<String>,
}
impl fmt::Debug for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Record([REDACTED])")
    }
}
impl Record {
    pub fn validate(&self, limits: Limits) -> Result<(), Error> {
        validate_name(&self.key)?;
        validate_name(&self.generation)?;
        if self.revision.0 == [0; 16]
            || self.value.len() > limits.value_bytes
            || self.indexes.len() > limits.indexes_per_record
        {
            return Err(Error::Invalid);
        }
        let mut seen = std::collections::BTreeSet::new();
        for index in &self.indexes {
            validate_name(index)?;
            if !seen.insert(index) {
                return Err(Error::Invalid);
            }
        }
        Ok(())
    }
    /// Stable lexical index member: deadline followed by a bounded ASCII key.
    pub fn position(&self) -> String {
        format!("{:016}:{}", self.deadline.0, self.key)
    }
}

#[derive(Clone)]
pub struct Write {
    pub expected: Option<Revision>,
    pub record: Record,
}
impl fmt::Debug for Write {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Write([REDACTED])")
    }
}

/// Exact records required to remain committed together at a read boundary.
/// This checks currentness only; it grants no domain disclosure authority.
#[derive(Clone, Debug)]
pub struct Check {
    pub key: String,
    pub revision: Revision,
    /// Exclusive authority deadline, no later than the observed record expiry.
    pub deadline: Deadline,
}

/// Internal expiring continuation bound to the exact scope and index, not a
/// snapshot or public token. Never expose its serde representation to clients;
/// the API owner supplies an opaque requester/query-bound cursor.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    scope: Scope,
    index: String,
    after: String,
    deadline: Deadline,
}
impl fmt::Debug for Cursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Cursor([REDACTED])")
    }
}
impl Cursor {
    pub fn new(scope: Scope, index: String, after: String, deadline: Deadline) -> Self {
        Self {
            scope,
            index,
            after,
            deadline,
        }
    }
    pub fn check(&self, scope: &Scope, index: &str, now: u64, limits: Limits) -> Result<(), Error> {
        if &self.scope != scope
            || self.index != index
            || self.after.len() > 145
            || self.after.len() < 18
            || self.after.as_bytes()[16] != b':'
            || !self.after.as_bytes()[..16].iter().all(u8::is_ascii_digit)
        {
            return Err(Error::Invalid);
        }
        validate_name(&self.after[17..])?;
        self.deadline.check(now, limits)
    }
    pub fn after(&self) -> &str {
        &self.after
    }
    pub fn deadline(&self) -> Deadline {
        self.deadline
    }
}

#[derive(Clone, Debug)]
pub struct Page {
    pub records: Vec<Record>,
    pub next: Option<Cursor>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Closed,
    Ready,
    Unavailable,
}

/// Shared scoped port. Operations are atomic; cancellation after network send
/// may commit, so callers reconcile current revisions instead of blind retries.
pub trait Store: Send + Sync {
    fn state(&self) -> State;
    fn get(&self, key: &str) -> impl Future<Output = Result<Option<Record>, Error>> + Send;
    /// Atomically check all revisions and exclusive deadlines without extending
    /// retention or changing any record. Used immediately before disclosure.
    fn check_current(&self, batch: &[Check]) -> impl Future<Output = Result<(), Error>> + Send;
    fn compare_exchange(&self, batch: &[Write]) -> impl Future<Output = Result<(), Error>> + Send;
    /// Delete only this generation. A delayed disconnect cannot delete a newer one.
    fn delete_generation(
        &self,
        generation: &str,
    ) -> impl Future<Output = Result<usize, Error>> + Send;
    fn page(
        &self,
        index: &str,
        cursor: Option<&Cursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Page, Error>> + Send;
    fn maintain(&self) -> impl Future<Output = Result<usize, Error>> + Send;
    fn close(&self);
}
impl<S: Store> Store for Arc<S> {
    fn state(&self) -> State {
        self.as_ref().state()
    }
    async fn get(&self, key: &str) -> Result<Option<Record>, Error> {
        self.as_ref().get(key).await
    }
    async fn check_current(&self, batch: &[Check]) -> Result<(), Error> {
        self.as_ref().check_current(batch).await
    }
    async fn compare_exchange(&self, batch: &[Write]) -> Result<(), Error> {
        self.as_ref().compare_exchange(batch).await
    }
    async fn delete_generation(&self, generation: &str) -> Result<usize, Error> {
        self.as_ref().delete_generation(generation).await
    }
    async fn page(
        &self,
        index: &str,
        cursor: Option<&Cursor>,
        limit: usize,
    ) -> Result<Page, Error> {
        self.as_ref().page(index, cursor, limit).await
    }
    async fn maintain(&self) -> Result<usize, Error> {
        self.as_ref().maintain().await
    }
    fn close(&self) {
        self.as_ref().close();
    }
}

/// Names are opaque bounded ASCII components, never raw identities or profile data.
pub fn validate_name(value: &str) -> Result<(), Error> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(Error::Invalid);
    }
    Ok(())
}

pub fn validate_batch(batch: &[Write], limits: Limits, now: u64) -> Result<(), Error> {
    if batch.is_empty() || batch.len() > limits.batch {
        return Err(Error::Invalid);
    }
    let mut seen = std::collections::BTreeSet::new();
    for write in batch {
        write.record.validate(limits)?;
        write.record.deadline.check(now, limits)?;
        if write.expected == Some(write.record.revision) || !seen.insert(&write.record.key) {
            return Err(Error::Invalid);
        }
    }
    Ok(())
}

/// Apply the same bounded read preconditions in every backend.
pub fn validate_checks(batch: &[Check], limits: Limits, now: u64) -> Result<(), Error> {
    if batch.is_empty() || batch.len() > limits.batch {
        return Err(Error::Invalid);
    }
    let mut seen = std::collections::BTreeSet::new();
    for check in batch {
        validate_name(&check.key)?;
        check.deadline.check(now, limits)?;
        if check.revision.0 == [0; 16] || !seen.insert(&check.key) {
            return Err(Error::Invalid);
        }
    }
    Ok(())
}
