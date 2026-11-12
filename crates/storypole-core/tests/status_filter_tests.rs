//! `--status` honours the archived filter too (`GB-295`).
//!
//! The defect these pin: the archived exclusion lived in the `else` branch of the status filter, so *any*
//! `--status` value turned it off. An archived ticket therefore answered `--status ready`, and `list` and
//! `list --status ready` disagreed by one row - which is the kind of contradiction that makes every count a
//! queue reports untrustworthy.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use storypole_core::StorypoleConfig;
use storypole_core::index::query::{TaskFilter, query_tasks};
use storypole_core::model::task::TaskStatus;
use storypole_core::ops::archive::archive_task;
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::store::StorypoleStore;
use tempfile::tempdir;

fn store(root: &Path) -> StorypoleStore {
    fs::create_dir_all(root.join(".gaugeblock").join("storypole").join("tickets")).expect("store");
    let store = StorypoleStore::init(root, StorypoleConfig::default()).expect("init");
    fs::remove_dir_all(root.join(".storypole")).ok();
    store
}

fn add(store: &mut StorypoleStore, title: &str) -> uuid::Uuid {
    add_task(
        store,
        CreateTaskParams {
            title: title.to_string(),
            ..Default::default()
        },
    )
    .expect("add")
    .id
}

#[test]
fn a_status_filter_hides_archived_tickets() {
    let tmp = tempdir().expect("tempdir");
    let mut store = store(tmp.path());
    let kept = add(&mut store, "Open work");
    let archived = add(&mut store, "Archived work");
    archive_task(&mut store, archived).expect("archive");

    let filter = TaskFilter {
        status: Some(TaskStatus::Ready),
        ..Default::default()
    };
    let found = query_tasks(store.conn(), &filter).expect("query");

    assert_eq!(
        found.len(),
        1,
        "the archived ticket is not offered as ready"
    );
    assert_eq!(found[0].id, kept);
}

#[test]
fn asking_for_archived_is_a_request_for_the_state() {
    let tmp = tempdir().expect("tempdir");
    let mut store = store(tmp.path());
    add(&mut store, "Open work");
    let archived = add(&mut store, "Archived work");
    archive_task(&mut store, archived).expect("archive");

    let filter = TaskFilter {
        status: Some(TaskStatus::Archived),
        ..Default::default()
    };
    let found = query_tasks(store.conn(), &filter).expect("query");
    assert_eq!(found.len(), 1, "the archived one is what was asked for");
    assert_eq!(found[0].id, archived);
}

#[test]
fn opting_in_shows_the_status_an_archived_ticket_earned() {
    // GB-179: archiving is a state, so the status survives it. `include_archived` is how a caller asks to
    // see that status rather than the fact it was retired.
    let tmp = tempdir().expect("tempdir");
    let mut store = store(tmp.path());
    let archived = add(&mut store, "Archived work");
    archive_task(&mut store, archived).expect("archive");

    let filter = TaskFilter {
        status: Some(TaskStatus::Ready),
        include_archived: true,
        ..Default::default()
    };
    let found = query_tasks(store.conn(), &filter).expect("query");
    assert_eq!(found.len(), 1, "an explicit opt-in still finds it");
}
