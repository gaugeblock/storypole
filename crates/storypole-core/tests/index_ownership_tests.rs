//! A store's index owns exactly its own tickets (`GB-355`).
//!
//! The defect these pin: the index rebuild was decided by a fingerprint of the *record files*, so an
//! index holding rows from somewhere else looked up to date forever. The harness index served the whole
//! suite's 374 rows while the harness owned 65, and a member's freshly verified status read stale in the
//! root's view. Both are the same missing question: is this file still about *this* store?

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use storypole_core::StorypoleConfig;
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::store::StorypoleStore;
use tempfile::tempdir;

/// A store in the suite layout with no tickets yet.
fn store(root: &Path) -> StorypoleStore {
    store_with_prefix(root, "SP")
}

/// The same, under a chosen ticket prefix, so a second store's keys cannot collide with this one's.
fn store_with_prefix(root: &Path, prefix: &str) -> StorypoleStore {
    // `tickets/` is what makes the store read records rather than the legacy ledger, and records mode is
    // where the suite lives (`GB-355` was measured there).
    fs::create_dir_all(root.join(".gaugeblock").join("storypole").join("tickets"))
        .expect("store dir");
    let config = StorypoleConfig {
        ticket_prefix: prefix.to_string(),
        ..StorypoleConfig::default()
    };
    let store = StorypoleStore::init(root, config).expect("init");
    fs::remove_dir_all(root.join(".storypole")).ok();
    store
}

fn add(store: &mut StorypoleStore, title: &str) {
    add_task(
        store,
        CreateTaskParams {
            title: title.to_string(),
            ..Default::default()
        },
    )
    .expect("add task");
}

/// A row inserted straight into the index, as a suite-era build would have left it: another store's
/// ticket, written into this file with no record behind it.
fn plant_foreign_row(store: &StorypoleStore, tmp: &Path) {
    let other_root = tmp.join("another-store");
    let mut other = store_with_prefix(&other_root, "ZZ");
    add(&mut other, "From another store");
    let foreign = storypole_core::index::query::get_all_tasks(other.conn())
        .expect("read the other store")
        .into_values()
        .next()
        .expect("one task");
    storypole_core::index::insert_task_record(store.conn(), &foreign).expect("plant a foreign row");
}

/// The ticket keys the store's index answers with.
fn keys(store: &StorypoleStore) -> Vec<String> {
    let mut all: Vec<String> = storypole_core::index::query::get_all_tasks(store.conn())
        .expect("read the store")
        .into_values()
        .map(|task| task.ticket_key)
        .collect();
    all.sort();
    all
}

#[test]
fn a_foreign_row_is_purged_the_next_time_the_store_opens() {
    let tmp = tempdir().expect("tempdir");
    let mut store = store(tmp.path());
    add(&mut store, "Real work");
    let expected = keys(&store);
    assert_eq!(expected.len(), 1, "one ticket to begin with");

    plant_foreign_row(&store, tmp.path());
    assert_eq!(keys(&store).len(), 2, "the row really is in the file");

    // Reopening is what a command does, and the row count no longer matches what this store built.
    let reopened = StorypoleStore::open(tmp.path()).expect("reopen");
    assert_eq!(
        keys(&reopened),
        expected,
        "the index holds this store's tickets and nothing else"
    );
}

#[test]
fn deleting_the_index_changes_no_answer() {
    // The invariant SP-36 states, made true for the store scope: the index is disposable, so a rebuild
    // from the records must answer identically.
    let tmp = tempdir().expect("tempdir");
    let mut store = store(tmp.path());
    add(&mut store, "First");
    add(&mut store, "Second");
    let before = keys(&store);

    let index = store.sqlite_path();
    drop(store);
    fs::remove_file(&index).expect("delete the index");

    let rebuilt = StorypoleStore::open(tmp.path()).expect("reopen without an index");
    assert_eq!(keys(&rebuilt), before);
}
