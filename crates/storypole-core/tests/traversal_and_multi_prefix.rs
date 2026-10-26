//! Integration tests for upward directory traversal and multi-prefix index resolution.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use storypole_core::config::StorypoleConfig;
use storypole_core::error::StorypoleError;
use storypole_core::index::query::get_max_ticket_number;
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::store::StorypoleStore;

#[test]
fn test_upward_directory_traversal() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo_dir = tmp.path().join("my_repo");
    let deep_nested = repo_dir.join("crates").join("core").join("src");
    fs::create_dir_all(&deep_nested).expect("create_dir_all");

    let config = StorypoleConfig {
        ticket_prefix: "MTR".into(),
        claim_ttl_minutes: 60,
        ..Default::default()
    };
    StorypoleStore::init(&repo_dir, config).expect("init");

    // Open from deep nested directory
    let store = StorypoleStore::open(&deep_nested).expect("open nested");
    assert_eq!(store.root_dir(), repo_dir.as_path());
    assert_eq!(store.config().ticket_prefix, "MTR");
}

#[test]
fn test_uninitialized_directory_returns_error() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let empty_dir = tmp.path().join("isolated");
    fs::create_dir_all(&empty_dir).expect("create_dir_all");

    let res = StorypoleStore::open(&empty_dir);
    assert!(matches!(res, Err(StorypoleError::Uninitialized(_))));
}

#[test]
fn test_multi_prefix_ticket_numbers() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = StorypoleConfig {
        ticket_prefix: "BW".into(),
        claim_ttl_minutes: 60,
        ..Default::default()
    };
    let mut store = StorypoleStore::init(tmp.path(), config).expect("init");

    let task1 = add_task(
        &mut store,
        CreateTaskParams {
            title: "Task 1".into(),
            ..Default::default()
        },
    )
    .expect("task1");
    assert_eq!(task1.ticket_key, "BW-1");

    let task2 = add_task(
        &mut store,
        CreateTaskParams {
            title: "Task 2".into(),
            ..Default::default()
        },
    )
    .expect("task2");
    assert_eq!(task2.ticket_key, "BW-2");

    // Check max ticket numbers for different prefixes
    assert_eq!(
        get_max_ticket_number(store.conn(), "BW").expect("get_max"),
        2
    );
    assert_eq!(
        get_max_ticket_number(store.conn(), "MTR").expect("get_max"),
        0
    );
    assert_eq!(
        get_max_ticket_number(store.conn(), "GB").expect("get_max"),
        0
    );
}

