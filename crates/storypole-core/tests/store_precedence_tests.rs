//! Single-store precedence, pinned by a fixture that would fail against a workspace-first read (`GB-241`).
//!
//! Two stores answer for two different sets of work: a member repository's own store, and the suite's at the
//! root. The rule is that the *nearest* store that actually holds tickets wins, because that is the one
//! whose branch is checked out - and the failure mode is silent: resolving the suite first makes a member's
//! work look absent while the command still succeeds.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use storypole_core::StorypoleConfig;
use storypole_core::index::query::{get_all_tasks, resolve_task_id};
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::store::StorypoleStore;

/// A store in the suite layout, under a chosen prefix so its keys cannot collide with the other store's.
fn store_at(root: &Path, prefix: &str) -> StorypoleStore {
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

fn add(store: &mut StorypoleStore, title: &str) -> String {
    add_task(
        store,
        CreateTaskParams {
            title: title.to_string(),
            ..Default::default()
        },
    )
    .expect("add")
    .ticket_key
}

#[test]
fn a_member_resolves_its_own_store_and_not_the_suite() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let suite_root = tmp.path();
    let mut suite = store_at(suite_root, "GB");
    let suite_key = add(&mut suite, "Suite work");

    // The member is a real checkout as far as the suite is concerned, and holds its own tickets.
    let member_root = suite_root.join("member-a");
    fs::create_dir_all(member_root.join(".git")).expect("member checkout");
    fs::write(
        suite_root.join(".gaugeblock").join("config.toml"),
        "[suite]\nmembers = [\"member-a\"]\n",
    )
    .expect("suite roster");
    let mut member = store_at(&member_root, "MA");
    let member_key = add(&mut member, "Member work");

    // Opened from inside the member: the member's store answers, and the suite's key is not visible.
    let from_member = StorypoleStore::open(&member_root).expect("open member");
    assert!(
        resolve_task_id(from_member.conn(), &member_key).is_ok(),
        "the member's own ticket resolves from the member"
    );
    assert!(
        resolve_task_id(from_member.conn(), &suite_key).is_err(),
        "and the suite's ticket does not, because this is not the suite's store"
    );
    assert_eq!(
        get_all_tasks(from_member.conn()).expect("tasks").len(),
        1,
        "a member answers for itself"
    );

    // Opened from the suite root: the suite's store answers.
    let from_suite = StorypoleStore::open(suite_root).expect("open suite");
    assert!(
        resolve_task_id(from_suite.conn(), &suite_key).is_ok(),
        "the suite's own ticket resolves from the suite"
    );
}
