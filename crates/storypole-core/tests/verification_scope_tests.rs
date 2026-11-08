//! `verify` refuses to certify member work the ticket does not claim (`GB-316`).
//!
//! The failure this guards against is quiet: a task naming `miter/**` with no scope ran the harness
//! chain, in the harness, and recorded a receipt with exit zero. `GB-272`, `GB-273`, `GB-275` and
//! `GB-276` were all certified that way, and every field in those receipts was true except what it was
//! about.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use storypole_core::config::StorypoleConfig;
use storypole_core::error::StorypoleError;
use storypole_core::model::task::TaskScope;
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::ops::verification::verify_task;
use storypole_core::store::StorypoleStore;
use tempfile::tempdir;

fn fixture(root: &Path) -> StorypoleStore {
    fs::create_dir_all(root.join("miter").join("src")).expect("member dir");
    fs::write(root.join("miter").join("Cargo.toml"), "[workspace]\n").expect("manifest");
    fs::create_dir_all(root.join(".gaugeblock").join("storypole")).expect("store dir");
    let config = StorypoleConfig {
        verification_commands: vec![
            ("miter".to_string(), "true".to_string()),
            ("default".to_string(), "true".to_string()),
        ],
        ..StorypoleConfig::default()
    };
    let store = StorypoleStore::init(root, config).expect("init");
    fs::remove_dir_all(root.join(".storypole")).ok();
    store
}

#[test]
fn an_unscoped_member_ticket_is_refused_and_the_candidate_is_named() {
    let tmp = tempdir().expect("tempdir");
    let mut store = fixture(tmp.path());

    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Names miter, claims nothing".into(),
            scope: TaskScope {
                target_files: vec!["miter/src/lib.rs".into()],
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("add task");

    let error = verify_task(&mut store, task.id, None, true, false)
        .expect_err("member work with no scope must not be certified");

    match error {
        StorypoleError::UnscopedMemberWork { ticket, repo, .. } => {
            assert_eq!(ticket, task.ticket_key);
            assert_eq!(
                repo, "miter",
                "the refusal names where the work actually is"
            );
        }
        other => panic!("expected a scope refusal, got {other:?}"),
    }
}

#[test]
fn a_refusal_leaves_the_task_unverified() {
    // The refusal has to happen *before* the ledger write, or it certifies and reports a failure.
    let tmp = tempdir().expect("tempdir");
    let mut store = fixture(tmp.path());

    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Names miter, claims nothing".into(),
            scope: TaskScope {
                target_files: vec!["miter/src/lib.rs".into()],
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("add task");

    verify_task(&mut store, task.id, None, true, false).expect_err("refused");

    let after = storypole_core::index::query::get_task_by_id(store.conn(), task.id).expect("read");
    assert!(
        after.verification_receipt.is_none(),
        "a refused verification must not leave a receipt behind"
    );
}

#[test]
fn harness_work_is_still_certified() {
    // The refusal is about unclaimed member work, not about demanding a scope for everything: a ticket
    // that names no member is harness-level work and belongs exactly where it is.
    let tmp = tempdir().expect("tempdir");
    let mut store = fixture(tmp.path());

    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Harness work".into(),
            ..Default::default()
        },
    )
    .expect("add task");

    let receipt =
        verify_task(&mut store, task.id, None, true, false).expect("harness work verifies");
    assert_eq!(receipt.exit_code, Some(0));
    assert_eq!(receipt.repo, None);
}
