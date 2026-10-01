#![cfg(feature = "conformance")]
use cmmr::{conformance::*, *};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn storage_contract() {
    futures::executor::block_on(async {
        let clock = TestClock::new(2_000_000_000_000);
        let a = Memory::new(Scope::new("a", "boot").unwrap(), limits(), clock.clone()).unwrap();
        let b = Memory::new(Scope::new("b", "boot").unwrap(), limits(), clock.clone()).unwrap();
        run(&a, &b, &clock).await;
        let restarted = Memory::new(Scope::new("a", "boot").unwrap(), limits(), clock).unwrap();
        assert!(restarted.get("entry").await.unwrap().is_none());
    });
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn revision_race_has_one_winner() {
    use std::sync::{Arc, Barrier};
    let store = Arc::new(
        Memory::new(
            Scope::new("race", "boot").unwrap(),
            limits(),
            TestClock::new(1000),
        )
        .unwrap(),
    );
    let barrier = Arc::new(Barrier::new(16));
    let joins: Vec<_> = (1..=16)
        .map(|n| {
            let store = store.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                futures::executor::block_on(store.compare_exchange(&[write("key", "g", n, 2000)]))
            })
        })
        .collect();
    assert_eq!(
        joins
            .into_iter()
            .map(|j| j.join().unwrap())
            .filter(Result::is_ok)
            .count(),
        1
    );
}
