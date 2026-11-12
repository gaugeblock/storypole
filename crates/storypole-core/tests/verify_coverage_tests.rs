//! A receipt names the tree it proved, and verification refuses to name a tree it never touched (`GB-313`).
//!
//! The defect these pin: `receipt.repo` came from the ticket's *scope* while the chain ran wherever
//! `resolve_target_dir` landed, and that function falls back to the store root when `root/<repo>` is
//! absent. A ticket scoped to a repository that is not checked out therefore verified against the harness,
//! and its receipt claimed the missing repository - nothing in the record said so.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use storypole_core::StorypoleConfig;
use storypole_core::error::StorypoleError;
use storypole_core::model::task::TaskScope;
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::ops::revision::coverage_line;
use storypole_core::ops::verification::verify_task;
use storypole_core::store::StorypoleStore;
use tempfile::tempdir;

fn store_with_member(root: &Path) -> StorypoleStore {
    fs::create_dir_all(root.join(".gaugeblock").join("storypole").join("tickets")).expect("store");
    let store = StorypoleStore::init(root, StorypoleConfig::default()).expect("init");
    fs::remove_dir_all(root.join(".storypole")).ok();
    // `absent-member` is declared but not checked out; `present-member` is both.
    fs::write(
        root.join(".gaugeblock").join("config.toml"),
        "[suite]\nmembers = [\"absent-member\", \"present-member\"]\n",
    )
    .expect("suite roster");
    fs::create_dir_all(root.join("present-member")).expect("member dir");
    store
}

#[test]
fn verifying_against_a_repository_that_is_not_checked_out_is_refused() {
    let tmp = tempdir().expect("tempdir");
    let mut store = store_with_member(tmp.path());
    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Work in a repository that is not here".into(),
            scope: TaskScope {
                repo: Some("absent-member".into()),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("task");

    let error = verify_task(&mut store, task.id, None, false, false)
        .expect_err("a chain run in the wrong tree must not be certified");
    match error {
        StorypoleError::RepositoryNotCheckedOut { repo, path, .. } => {
            assert_eq!(repo, "absent-member");
            assert!(path.ends_with("absent-member"));
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_receipt_names_the_repository_whose_chain_ran() {
    let tmp = tempdir().expect("tempdir");
    let mut store = store_with_member(tmp.path());
    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Work in a repository that is here".into(),
            scope: TaskScope {
                repo: Some("present-member".into()),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("task");

    let receipt = verify_task(&mut store, task.id, None, false, false).expect("verify");
    assert_eq!(receipt.repo.as_deref(), Some("present-member"));

    let stored = storypole_core::index::query::get_task_by_id(store.conn(), task.id).expect("get");
    let coverage = coverage_line(&stored).expect("a verified ticket has a coverage line");
    assert!(
        coverage.contains("present-member"),
        "the line names the repository: {coverage}"
    );
    assert!(
        coverage.contains("ticket scope says present-member"),
        "and puts it beside the declared scope: {coverage}"
    );
}
