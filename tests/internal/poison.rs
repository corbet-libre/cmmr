use super::*;

#[test]
fn real_mutex_poisoning_fences_reads_and_close() {
    struct Time;
    impl Clock for Time {
        fn now(&self) -> Result<u64, Error> {
            Ok(100)
        }
    }
    let store = Memory::new(
        Scope::new("a", "boot").unwrap(),
        Limits {
            records: 1,
            value_bytes: 10,
            batch: 1,
            indexes_per_record: 1,
            page: 1,
            ttl_ms: 100,
        },
        Time,
    )
    .unwrap();
    futures::executor::block_on(store.compare_exchange(&[Write {
        expected: None,
        record: Record {
            key: "key".into(),
            generation: "generation".into(),
            revision: Revision([1; 16]),
            deadline: Deadline::new(150).unwrap(),
            value: vec![42],
            indexes: vec!["index".into()],
        },
    }]))
    .unwrap();
    assert_eq!(store.state(), State::Ready);
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = store.data.lock().unwrap();
        panic!("induced mutation interruption");
    }));
    assert!(failure.is_err());
    assert_eq!(store.state(), State::Unavailable);
    assert_eq!(
        futures::executor::block_on(store.get("key")),
        Err(Error::Unavailable)
    );
    store.close();
    assert_eq!(store.state(), State::Unavailable);
    let data = store
        .data
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert!(data.records.is_empty());
    assert!(data.indexes.is_empty());
    assert!(data.generations.is_empty());
    assert!(data.expiries.is_empty());
    assert!(data.closed);
}
