//! Concurrent `defer` calls must not claim the same ticket key (`GB-185`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::sync::{Arc, Barrier};

use storypole_core::config::StorypoleConfig;
use storypole_core::model::TaskScope;
use storypole_core::model::task::TaskKind;
use storypole_core::ops::graph::defer_issue;
use storypole_core::store::StorypoleStore;

#[test]
fn concurrent_defers_allocate_distinct_ticket_keys() {
    let dir = tempfile::tempdir().expect("tempdir");
    StorypoleStore::init(dir.path(), StorypoleConfig::default()).expect("init");

    // Eight writers released together, each opening its own store: the key comes from the index, so
    // without a cross-process lock they all read the same maximum.
    let writers = 8;
    let barrier = Arc::new(Barrier::new(writers));
    let handles: Vec<_> = (0..writers)
        .map(|n| {
            let root = dir.path().to_path_buf();
            let gate = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let mut store = StorypoleStore::open(&root).expect("open");
                gate.wait();
                defer_issue(
                    &mut store,
                    format!("concurrent defer {n}"),
                    "body".to_string(),
                    TaskKind::Bug,
                    TaskScope::default(),
                    None,
                    "test".to_string(),
                )
                .expect("defer")
                .ticket_key
            })
        })
        .collect();

    let keys: Vec<String> = handles
        .into_iter()
        .map(|handle| handle.join().expect("thread"))
        .collect();

    let unique: BTreeSet<&String> = keys.iter().collect();
    assert_eq!(
        unique.len(),
        writers,
        "duplicate ticket keys allocated: {keys:?}"
    );
}
