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
}
