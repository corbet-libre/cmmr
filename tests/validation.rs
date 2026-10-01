#![cfg(feature = "conformance")]
use cmmr::{conformance::*, *};
use std::sync::Arc;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn bounds_and_serialization_refuse_invalid_storage_inputs() {
    for mode in 0..11 {
        let mut value = limits();
        match mode {
            0 => value.records = 0,
            1 => value.records = 1_000_001,
            2 => value.value_bytes = 0,
            3 => value.value_bytes = 1_048_577,
            4 => value.batch = 0,
            5 => value.batch = 257,
            6 => value.indexes_per_record = 33,
            7 => value.page = 0,
            8 => value.page = 1025,
            9 => value.ttl_ms = 0,
            _ => value.ttl_ms = 31_536_000_001,
        }
        assert_eq!(value.validate(), Err(Error::Invalid));
        assert!(matches!(
            Memory::new(Scope::new("a", "boot").unwrap(), value, TestClock::new(100)),
            Err(Error::Invalid)
        ));
    }
    assert!(
        Limits {
            records: 1_000_000,
            value_bytes: 1_048_576,
            batch: 256,
            indexes_per_record: 32,
            page: 1024,
            ttl_ms: 31_536_000_000
        }
        .validate()
        .is_ok()
    );
    for bad in [
        String::new(),
        "x".repeat(129),
        "bad:name".into(),
        "café".into(),
    ] {
        assert_eq!(validate_name(&bad), Err(Error::Invalid));
        assert!(Scope::new(&bad, "boot").is_err());
        assert!(Scope::new("a", &bad).is_err());
    }
    assert!(validate_name("A_z-09").is_ok());
    assert!(validate_name(&"a".repeat(128)).is_ok());
    for value in [0, MAX_TIME + 1] {
        assert_eq!(Deadline::new(value), Err(Error::Invalid));
        assert!(serde_json::from_str::<Deadline>(&value.to_string()).is_err());
    }
    let deadline = Deadline::new(200).unwrap();
    assert_eq!(u64::from(deadline), 200);
    assert_eq!(serde_json::to_string(&deadline).unwrap(), "200");
    assert_eq!(serde_json::from_str::<Deadline>("200").unwrap(), deadline);
    for input in [
        r#"{"community":"!","incarnation":"boot"}"#,
        r#"{"community":"a","incarnation":"!"}"#,
    ] {
        let scope: Scope = serde_json::from_str(input).unwrap();
        assert!(matches!(
            Memory::new(scope, limits(), TestClock::new(100)),
            Err(Error::Invalid)
        ));
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn invalid_batches_never_change_live_records_or_indexes() {
    futures::executor::block_on(async {
        let store = Memory::new(
            Scope::new("a", "boot").unwrap(),
            limits(),
            TestClock::new(100),
        )
        .unwrap();
        let original = write("entry", "generation", 1, 200);
        store
            .compare_exchange(std::slice::from_ref(&original))
            .await
            .unwrap();
        for mode in 0..7 {
            let mut bad = write("other", "generation", 2, 200);
            match mode {
                0 => bad.record.key = "!".into(),
                1 => bad.record.generation = "!".into(),
                2 => bad.record.revision = Revision([0; 16]),
                3 => bad.record.value = vec![0; 1025],
                4 => bad.record.indexes = vec!["one".into(), "two".into(), "three".into()],
                5 => bad.record.indexes = vec!["!".into()],
                _ => bad.record.indexes = vec!["same".into(), "same".into()],
            }
            assert_eq!(store.compare_exchange(&[bad]).await, Err(Error::Invalid));
        }
        assert_eq!(store.compare_exchange(&[]).await, Err(Error::Invalid));
        assert_eq!(
            store
                .compare_exchange(&vec![write("other", "g", 2, 200); 5])
                .await,
            Err(Error::Invalid)
        );
        let mut unchanged = original.clone();
        unchanged.expected = Some(original.record.revision);
        assert_eq!(
            store.compare_exchange(&[unchanged]).await,
            Err(Error::Invalid)
        );
        assert_eq!(
            store
                .compare_exchange(&[write("other", "g", 2, 200), write("other", "g", 3, 200)])
                .await,
            Err(Error::Invalid)
        );
        assert!(store.get("!").await.is_err());
        assert!(store.delete_generation("!").await.is_err());
        assert!(store.page("!", None, 1).await.is_err());
        assert!(store.page("visible", None, 0).await.is_err());
        assert!(store.page("visible", None, 3).await.is_err());
        assert_eq!(
            store.get("entry").await.unwrap(),
            Some(original.record.clone())
        );
        assert_eq!(
            store.page("visible", None, 2).await.unwrap().records,
            vec![original.record]
        );
    });
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn cursor_encoding_and_deadlines_are_checked_before_paging() {
    let scope = Scope::new("a", "boot").unwrap();
    let position = write("key", "g", 1, 200).record.position();
    let cursor = Cursor::new(
        scope.clone(),
        "visible".into(),
        position.clone(),
        Deadline::new(300).unwrap(),
    );
    assert!(cursor.check(&scope, "visible", 100, limits()).is_ok());
    assert_eq!(cursor.after(), position);
    assert_eq!(cursor.deadline().unix_ms(), 300);
    for mode in 0..9 {
        let mut bound = scope.clone();
        let mut index = "visible".to_string();
        let mut after = position.clone();
        let mut end = 300;
        match mode {
            0 => bound = Scope::new("b", "boot").unwrap(),
            1 => index = "other".into(),
            2 => after = "a".repeat(146),
            3 => after.clear(),
            4 => after = "0000000000000200;key".into(),
            5 => after = "x000000000000200:key".into(),
            6 => after = "0000000000000200:!".into(),
            7 => end = 100,
            _ => end = 60_101,
        }
        let bad = Cursor::new(bound, index, after, Deadline::new(end).unwrap());
        assert!(bad.check(&scope, "visible", 100, limits()).is_err());
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn shared_arc_adapters_use_the_same_real_contract_and_redact_values() {
    futures::executor::block_on(async {
        let time = TestClock::new(1000);
        let clock = Arc::new(time.clone());
        assert_eq!(Clock::now(&clock).unwrap(), 1000);
        let a = Arc::new(
            Memory::new(Scope::new("a", "boot").unwrap(), limits(), clock.clone()).unwrap(),
        );
        let b = Arc::new(Memory::new(Scope::new("b", "boot").unwrap(), limits(), clock).unwrap());
        assert_eq!(Store::state(&a), State::Ready);
        assert_eq!(Store::maintain(&a).await.unwrap(), 0);
        assert_eq!(format!("{a:?}"), "Memory([REDACTED])");
        let value = write("private_name", "private_generation", 9, 2000);
        let cursor = Cursor::new(
            Scope::new("a", "boot").unwrap(),
            "visible".into(),
            value.record.position(),
            Deadline::new(2000).unwrap(),
        );
        assert_eq!(
            format!("{:?}", Scope::new("private", "private").unwrap()),
            "Scope([REDACTED])"
        );
        assert_eq!(
            format!("{:?}", value.record.revision),
            "Revision([REDACTED])"
        );
        assert_eq!(format!("{:?}", value.record), "Record([REDACTED])");
        assert_eq!(format!("{value:?}"), "Write([REDACTED])");
        assert_eq!(format!("{cursor:?}"), "Cursor([REDACTED])");
        assert_eq!(Error::Corrupt.to_string(), "volatile storage: Corrupt");
        run(&a, &b, &time).await;
        assert_eq!(Store::state(&a), State::Closed);
    });
}

struct UnavailableClock;
impl Clock for UnavailableClock {
    fn now(&self) -> Result<u64, Error> {
        Err(Error::Unavailable)
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn invalid_or_unavailable_clock_never_changes_records() {
    futures::executor::block_on(async {
        let unavailable =
            Memory::new(Scope::new("a", "boot").unwrap(), limits(), UnavailableClock).unwrap();
        assert_eq!(unavailable.get("key").await, Err(Error::Unavailable));
        let clock = TestClock::new(MAX_TIME + 1);
        let store = Memory::new(Scope::new("a", "boot").unwrap(), limits(), clock.clone()).unwrap();
        assert_eq!(store.get("key").await, Err(Error::Invalid));
        clock.set(MAX_TIME);
        assert!(matches!(
            store.page("visible", None, 1).await,
            Err(Error::Invalid)
        ));
    });
}
