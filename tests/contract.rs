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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn full_capacity_replacements_stay_atomic_then_succeed() {
    futures::executor::block_on(async {
        let start = 3_000_000_000_000;
        let clock = TestClock::new(start);
        let store =
            Memory::new(Scope::new("cap", "boot").unwrap(), limits(), clock.clone()).unwrap();
        let seed = |key: &str, revision: u8| {
            let mut pending = write(key, "g1", revision, start + 5000);
            pending.record.value = vec![revision];
            pending
        };
        let originals = [
            seed("r1", 11),
            seed("r2", 12),
            seed("r3", 13),
            seed("r4", 14),
        ];
        store.compare_exchange(&originals).await.unwrap();
        let mut first = write("r1", "g2", 21, start + 6000);
        first.expected = Some(Revision([11; 16]));
        first.record.indexes = vec!["fresh".into()];
        first.record.value = vec![21];
        let mut second = write("r2", "g2", 22, start + 6000);
        second.expected = Some(Revision([12; 16]));
        second.record.indexes = vec!["fresh".into()];
        second.record.value = vec![22];
        let mut stale = write("r3", "g2", 23, start + 6000);
        stale.expected = Some(Revision([99; 16]));
        stale.record.indexes = vec!["fresh".into()];
        stale.record.value = vec![23];
        assert_eq!(
            store.compare_exchange(&[first, second, stale]).await,
            Err(Error::Conflict)
        );
        for original in &originals {
            assert_eq!(
                store.get(&original.record.key).await.unwrap(),
                Some(original.record.clone())
            );
        }
        let first_page = store.page("visible", None, 2).await.unwrap();
        assert_eq!(
            first_page
                .records
                .iter()
                .map(|r| r.key.as_str())
                .collect::<Vec<_>>(),
            vec!["r1", "r2"]
        );
        let second_page = store
            .page("visible", first_page.next.as_ref(), 2)
            .await
            .unwrap();
        assert_eq!(
            second_page
                .records
                .iter()
                .map(|r| r.key.as_str())
                .collect::<Vec<_>>(),
            vec!["r3", "r4"]
        );
        assert!(
            store
                .page("fresh", None, 2)
                .await
                .unwrap()
                .records
                .is_empty()
        );
        assert_eq!(store.delete_generation("g2").await.unwrap(), 0);
        let mut replacement_one = write("r1", "g2", 31, start + 6000);
        replacement_one.expected = Some(Revision([11; 16]));
        replacement_one.record.indexes = vec!["fresh".into()];
        replacement_one.record.value = vec![31];
        let mut replacement_two = write("r2", "g2", 32, start + 6000);
        replacement_two.expected = Some(Revision([12; 16]));
        replacement_two.record.indexes = vec!["fresh".into()];
        replacement_two.record.value = vec![32];
        store
            .compare_exchange(&[replacement_one.clone(), replacement_two.clone()])
            .await
            .unwrap();
        assert_eq!(
            store.get("r1").await.unwrap(),
            Some(replacement_one.record.clone())
        );
        assert_eq!(
            store.get("r2").await.unwrap(),
            Some(replacement_two.record.clone())
        );
        assert_eq!(
            store.get("r3").await.unwrap(),
            Some(originals[2].record.clone())
        );
        assert_eq!(
            store.page("visible", None, 2).await.unwrap().records,
            vec![originals[2].record.clone(), originals[3].record.clone()]
        );
        assert_eq!(
            store.page("fresh", None, 2).await.unwrap().records,
            vec![replacement_one.record, replacement_two.record]
        );
        assert_eq!(
            store
                .compare_exchange(&[write("r5", "g1", 33, start + 6000)])
                .await,
            Err(Error::Capacity)
        );
        assert_eq!(store.get("r5").await.unwrap(), None);
    });
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn expiry_reclaims_capacity_and_generation_fencing_holds() {
    futures::executor::block_on(async {
        let start = 3_100_000_000_000;
        let clock = TestClock::new(start);
        let store =
            Memory::new(Scope::new("exp", "boot").unwrap(), limits(), clock.clone()).unwrap();
        let seed = |key: &str, generation: &str, revision: u8, deadline: u64| {
            let mut pending = write(key, generation, revision, deadline);
            pending.record.value = vec![revision];
            pending
        };
        store
            .compare_exchange(&[
                seed("e1", "old", 41, start + 1000),
                seed("e2", "old", 42, start + 1000),
                seed("k1", "old", 43, start + 5000),
                seed("k2", "old", 44, start + 5000),
            ])
            .await
            .unwrap();
        assert_eq!(
            store
                .compare_exchange(&[seed("late", "old", 45, start + 5000)])
                .await,
            Err(Error::Capacity)
        );
        clock.set(start + 1000);
        assert_eq!(store.maintain().await.unwrap(), 2);
        assert_eq!(store.get("e1").await.unwrap(), None);
        assert_eq!(store.get("e2").await.unwrap(), None);
        assert!(store.get("k1").await.unwrap().is_some());
        assert!(store.get("k2").await.unwrap().is_some());
        store
            .compare_exchange(&[seed("n1", "old", 51, start + 6000)])
            .await
            .unwrap();
        let mut moved = seed("k1", "kept", 55, start + 6000);
        moved.expected = Some(Revision([43; 16]));
        store.compare_exchange(&[moved.clone()]).await.unwrap();
        assert_eq!(store.delete_generation("old").await.unwrap(), 2);
        assert_eq!(store.get("k1").await.unwrap(), Some(moved.record.clone()));
        assert_eq!(store.get("k2").await.unwrap(), None);
        assert_eq!(store.get("n1").await.unwrap(), None);
        assert_eq!(
            store.page("visible", None, 2).await.unwrap().records,
            vec![moved.record]
        );
    });
}
