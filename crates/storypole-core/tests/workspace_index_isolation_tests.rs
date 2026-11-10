//! The suite view and a store view must not share one index file (`GB-353`).
//!
//! They did: `StorypoleWorkspaceStore` reused the root store's `index.sqlite` for the unified view, while
//! `StorypoleStore` rebuilt that same file from the root ledger alone. Whichever command ran last decided what a board
//! saw - a store-scoped command truncated the suite to the harness, and a failed workspace sync left a partial view that
//! looked like data loss. Nothing was ever lost; the file simply held whichever scope wrote it most recently.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use storypole_core::config::StorypoleConfig;
use storypole_core::index::query::{TaskFilter, query_tasks};
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::store::StorypoleStore;
use storypole_core::workspace::store::StorypoleWorkspaceStore;

/// A suite: a root store with one ticket, and one member with a ticket of its own.
fn suite() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    let mut root_store = StorypoleStore::init(root, StorypoleConfig::default()).expect("root init");
    add_task(
        &mut root_store,
        CreateTaskParams {
            title: "a harness ticket".into(),
            ..Default::default()
        },
    )
    .expect("root ticket");

    let member = root.join("member");
    fs::create_dir_all(&member).expect("member dir");
    let mut member_store = StorypoleStore::init(
        &member,
        StorypoleConfig {
            ticket_prefix: "MB".to_string(),
            ..Default::default()
        },
    )
    .expect("member init");
    add_task(
        &mut member_store,
        CreateTaskParams {
            title: "a member ticket".into(),
            ..Default::default()
        },
    )
    .expect("member ticket");

    dir
}

fn count(store: &StorypoleStore) -> usize {
    query_tasks(
        store.conn(),
        &TaskFilter {
            include_archived: true,
            ..Default::default()
        },
    )
    .expect("query")
    .len()
}

#[test]
fn a_store_view_does_not_truncate_the_suite_view() {
    let dir = suite();
    let root = dir.path();

    let workspace = StorypoleWorkspaceStore::open(root).expect("workspace");
    assert_eq!(
        count_workspace(&workspace),
        2,
        "the suite view holds every store"
    );

    let store = StorypoleStore::open(root).expect("root store");
    assert_eq!(count(&store), 1, "the store view holds only its own");

    // The regression: opening the store used to rebuild the workspace's file from the root ledger, leaving one row.
    let workspace_file = store.workspace_sqlite_path();
    assert_ne!(
        workspace_file,
        store.sqlite_path(),
        "the two scopes are different files"
    );
    let conn = rusqlite::Connection::open(&workspace_file).expect("open workspace file");
    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
        .expect("count");
    assert_eq!(
        rows, 2,
        "the suite view survives a store-scoped command touching its own index"
    );
}

#[test]
fn the_two_scopes_write_different_files() {
    let dir = suite();
    let root = dir.path();

    let workspace = StorypoleWorkspaceStore::open(root).expect("workspace");
    assert_eq!(count_workspace(&workspace), 2);
    let workspace_file = root.join(".gaugeblock/storypole/workspace.sqlite");
    assert!(workspace_file.is_file(), "the suite view has its own file");

    let store = StorypoleStore::open(root).expect("root store");
    assert!(store.sqlite_path().is_file(), "and the store keeps its own");
}

fn count_workspace(workspace: &StorypoleWorkspaceStore) -> usize {
    query_tasks(
        workspace.conn(),
        &TaskFilter {
            include_archived: true,
            ..Default::default()
        },
    )
    .expect("query")
    .len()
}
