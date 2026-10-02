//! The same acceptance vectors run against every real adapter.
use crate::*;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

#[derive(Clone)]
pub struct TestClock(Arc<AtomicU64>);
impl TestClock {
    pub fn new(now: u64) -> Self {
        Self(Arc::new(AtomicU64::new(now)))
    }
    pub fn set(&self, now: u64) {
        self.0.store(now, Ordering::SeqCst);
    }
}
impl Clock for TestClock {
    fn now(&self) -> Result<u64, Error> {
        Ok(self.0.load(Ordering::SeqCst))
    }
}
pub fn limits() -> Limits {
    Limits {
        records: 4,
        value_bytes: 1024,
        batch: 4,
        indexes_per_record: 2,
        page: 2,
        ttl_ms: 60_000,
    }
}
pub fn write(key: &str, generation: &str, revision: u8, deadline: u64) -> Write {
    Write {
        expected: None,
        record: Record {
            key: key.into(),
            generation: generation.into(),
            revision: Revision([revision; 16]),
            deadline: Deadline::new(deadline).unwrap(),
            value: vec![0, 255, 13],
            indexes: vec!["visible".into()],
        },
    }
}
/// Supply two independent scopes using the same physical backend and clock.
pub async fn run<A: Store, B: Store>(a: &A, b: &B, clock: &TestClock) {
    let start = clock.now().unwrap();
    let first = write("entry", "old", 1, start + 1000);
    a.compare_exchange(std::slice::from_ref(&first))
        .await
        .unwrap();
    assert_eq!(a.get("entry").await.unwrap(), Some(first.record.clone()));
    assert_eq!(b.get("entry").await.unwrap(), None);
    b.compare_exchange(&[write("entry", "old", 2, start + 2000)])
        .await
        .unwrap();
    assert_eq!(
        a.compare_exchange(std::slice::from_ref(&first)).await,
        Err(Error::Conflict)
    );
    let mut changed = write("entry", "new", 3, start + 1500);
    changed.expected = Some(first.record.revision);
    changed.record.indexes = vec!["new-index".into()];
    a.compare_exchange(std::slice::from_ref(&changed))
        .await
        .unwrap();
    assert!(a.page("visible", None, 2).await.unwrap().records.is_empty());
    assert_eq!(a.page("new-index", None, 2).await.unwrap().records.len(), 1);
    assert_eq!(a.delete_generation("old").await.unwrap(), 0);
    assert_eq!(b.delete_generation("old").await.unwrap(), 1);
    let mut bad = write("entry", "new", 4, start + 1500);
    bad.expected = Some(first.record.revision);
    assert_eq!(
        a.compare_exchange(&[write("orphan", "g", 5, start + 1500), bad])
            .await,
        Err(Error::Conflict)
    );
    assert_eq!(a.get("orphan").await.unwrap(), None);
    assert!(a.page("visible", None, 2).await.unwrap().records.is_empty());
    // Polling is the mutation boundary; a never-polled future has no effects.
    let pending = [write("cancelled", "g", 6, start + 1500)];
    drop(a.compare_exchange(&pending));
    assert_eq!(a.get("cancelled").await.unwrap(), None);
    assert_eq!(
        a.compare_exchange(&[write("dead", "g", 7, start)]).await,
        Err(Error::Expired)
    );
    assert_eq!(
        a.compare_exchange(&[write("long", "g", 7, start + 60_001)])
            .await,
        Err(Error::Invalid)
    );
    let mut oversized = write("large", "g", 7, start + 1000);
    oversized.record.value = vec![0; 1025];
    assert_eq!(a.compare_exchange(&[oversized]).await, Err(Error::Invalid));
    a.compare_exchange(&[
        write("a", "g", 8, start + 1000),
        write("b", "g", 9, start + 1000),
        write("c", "g", 10, start + 1000),
    ])
    .await
    .unwrap();
    assert_eq!(
        a.compare_exchange(&[write("full", "g", 11, start + 1000)])
            .await,
        Err(Error::Capacity)
    );
    assert_eq!(a.get("entry").await.unwrap(), Some(changed.record.clone()));
    let page = a.page("visible", None, 2).await.unwrap();
    assert_eq!(
        page.records
            .iter()
            .map(|r| r.key.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    let cursor = page.next.unwrap();
    assert_eq!(
        a.page("visible", Some(&cursor), 2).await.unwrap().records[0].key,
        "c"
    );
    assert!(matches!(
        b.page("visible", Some(&cursor), 2).await,
        Err(Error::Invalid)
    ));
    assert!(matches!(
        a.page("other", Some(&cursor), 2).await,
        Err(Error::Invalid)
    ));
    assert!(matches!(
        a.page("visible", None, 3).await,
        Err(Error::Invalid)
    ));
    // One atomic cut across both records, without refreshing a lease or token.
    let checks = [
        Check {
            key: "a".into(),
            revision: Revision([8; 16]),
            deadline: Deadline::new(start + 1000).unwrap(),
        },
        Check {
            key: "b".into(),
            revision: Revision([9; 16]),
            deadline: Deadline::new(start + 1000).unwrap(),
        },
    ];
    a.check_current(&checks).await.unwrap();
    assert_eq!(
        a.get("a").await.unwrap().unwrap(),
        write("a", "g", 8, start + 1000).record
    );
    assert_eq!(b.check_current(&checks).await, Err(Error::Conflict));
    assert_eq!(a.check_current(&[]).await, Err(Error::Invalid));
    assert_eq!(
        a.check_current(&[checks[0].clone(), checks[0].clone()])
            .await,
        Err(Error::Invalid)
    );
    assert_eq!(
        a.check_current(&vec![checks[0].clone(); 5]).await,
        Err(Error::Invalid)
    );
    for check in [
        Check {
            key: "invalid/".into(),
            ..checks[0].clone()
        },
        Check {
            revision: Revision([0; 16]),
            ..checks[0].clone()
        },
        Check {
            deadline: Deadline::new(start + 60_001).unwrap(),
            ..checks[0].clone()
        },
    ] {
        assert_eq!(a.check_current(&[check]).await, Err(Error::Invalid));
    }
    for check in [
        Check {
            key: "missing".into(),
            ..checks[0].clone()
        },
        Check {
            revision: Revision([7; 16]),
            ..checks[0].clone()
        },
        Check {
            deadline: Deadline::new(start + 1001).unwrap(),
            ..checks[0].clone()
        },
    ] {
        assert_eq!(
            a.check_current(&[checks[1].clone(), check]).await,
            Err(Error::Conflict)
        );
    }
    clock.set(start + 999);
    a.check_current(&checks).await.unwrap();
    assert!(a.get("a").await.unwrap().is_some());
    clock.set(start + 1000);
    assert_eq!(a.check_current(&checks).await, Err(Error::Expired));
    assert_eq!(a.get("a").await.unwrap(), None);
    assert!(a.page("visible", None, 2).await.unwrap().records.is_empty());
    // A stale heartbeat cannot renew a dead generation.
    let mut late = write("a", "g", 12, start + 2000);
    late.expected = Some(Revision([8; 16]));
    assert_eq!(a.compare_exchange(&[late]).await, Err(Error::Conflict));
    clock.set(start + 999);
    assert_eq!(a.get("entry").await, Err(Error::ClockRegression));
    clock.set(start + 1500);
    assert_eq!(a.get("entry").await.unwrap(), None);
    assert_eq!(a.delete_generation("new").await.unwrap(), 0);
    a.compare_exchange(&[write("again", "g", 13, start + 2000)])
        .await
        .unwrap();
    assert_eq!(a.delete_generation("g").await.unwrap(), 1);
    assert!(a.page("visible", None, 2).await.unwrap().records.is_empty());
    clock.set(start + 60_000);
    assert!(matches!(
        a.page("visible", Some(&cursor), 2).await,
        Err(Error::Expired)
    ));
    a.close();
    assert_eq!(a.state(), State::Closed);
    assert_eq!(a.get("entry").await, Err(Error::Closed));
    assert_eq!(
        a.check_current(&[Check {
            deadline: Deadline::new(start + 60_001).unwrap(),
            ..checks[0].clone()
        }])
        .await,
        Err(Error::Closed)
    );
}
