//! Deliberate status transitions: park, unpark, reopen (`SP-47`).
//!
//! The defect these pin: `Deferred` was reachable only at creation, so `GB-155` carried an owner
//! do-not-start directive while reading `Ready` and `storypole next` kept proposing it. A `Verified`
//! task could not be reopened at all (`GB-341`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use storypole_core::StorypoleConfig;
use storypole_core::error::StorypoleError;
use storypole_core::model::task::TaskStatus;
use storypole_core::ops::graph::link_tasks;
use storypole_core::ops::lifecycle::{park_task, reopen_task, unpark_task};
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::ops::verification::verify_task;
use storypole_core::store::StorypoleStore;
use tempfile::tempdir;

/// A store in the suite layout, with no verification chain configured so `verify` cannot need a
/// toolchain (`verify_task(.., false, ..)` records an attestation).
fn fixture(root: &Path) -> StorypoleStore {
    fs::create_dir_all(root.join(".gaugeblock").join("storypole")).expect("store dir");
    let store = StorypoleStore::init(root, StorypoleConfig::default()).expect("init");
    fs::remove_dir_all(root.join(".storypole")).ok();
    store
}

fn task(store: &mut StorypoleStore, title: &str) -> storypole_core::model::TaskNode {
    add_task(
        store,
        CreateTaskParams {
            title: title.to_string(),
            ..Default::default()
        },
    )
    .expect("add task")
}

#[test]
fn parking_removes_a_task_from_the_ready_queue_and_unparking_restores_it() {
    let tmp = tempdir().expect("tempdir");
    let mut store = fixture(tmp.path());
    let parked = task(&mut store, "Held work");

    assert_eq!(
        park_task(&mut store, parked.id, Some("owner hold")).expect("park"),
        TaskStatus::Deferred
    );
    let stored =
        storypole_core::index::query::get_task_by_id(store.conn(), parked.id).expect("get");
    assert_eq!(stored.status, TaskStatus::Deferred, "the status moved");
    assert_eq!(
        stored.priority, parked.priority,
        "parking preserves the priority it will return with"
    );

    assert_eq!(
        unpark_task(&mut store, parked.id).expect("unpark"),
        TaskStatus::Ready,
        "no dependencies, so it comes back ready"
    );
}

#[test]
fn parking_a_verified_task_is_refused_because_it_would_hide_finished_work() {
    let tmp = tempdir().expect("tempdir");
    let mut store = fixture(tmp.path());
    let done = task(&mut store, "Finished work");
    verify_task(&mut store, done.id, None, false, false).expect("verify");

    let error = park_task(&mut store, done.id, None).expect_err("verified work must not be parked");
    match error {
        StorypoleError::IllegalTransition { why, .. } => {
            assert!(
                why.contains("reopen"),
                "the refusal names the verb that would say it honestly, got: {why}"
            );
        }
        other => panic!("expected an illegal transition, got {other:?}"),
    }
}

#[test]
fn reopening_a_verified_task_keeps_the_receipt_that_certified_it() {
    let tmp = tempdir().expect("tempdir");
    let mut store = fixture(tmp.path());
    let done = task(&mut store, "Certified then disputed");
    let receipt = verify_task(&mut store, done.id, None, false, false).expect("verify");

    assert_eq!(
        reopen_task(&mut store, done.id, "the change it verified was reverted").expect("reopen"),
        TaskStatus::Ready
    );

    let stored = storypole_core::index::query::get_task_by_id(store.conn(), done.id).expect("get");
    assert_eq!(stored.status, TaskStatus::Ready);
    let kept = stored
        .verification_receipt
        .expect("the earlier receipt survives, so the record can say what was certified");
    assert_eq!(kept.verifier, receipt.verifier);
    assert_eq!(kept.verified_at_epoch_ms, receipt.verified_at_epoch_ms);
}

#[test]
fn unparking_a_task_with_an_open_dependency_returns_it_blocked_not_ready() {
    // `Ready` claims the task can be started. Handing back a parked ticket whose prerequisite is
    // unfinished is the same lie as a held epic reading Ready (`GB-155`).
    let tmp = tempdir().expect("tempdir");
    let mut store = fixture(tmp.path());
    let prerequisite = task(&mut store, "Unfinished prerequisite");
    let dependent = task(&mut store, "Waiting on the prerequisite");
    link_tasks(
        &mut store,
        dependent.id,
        prerequisite.id,
        storypole_core::model::task::EdgeKind::BlockedBy,
    )
    .expect("link");

    let parked = park_task(&mut store, dependent.id, None).expect("park while blocked");
    assert_eq!(parked, TaskStatus::Deferred);

    assert_eq!(
        unpark_task(&mut store, dependent.id).expect("unpark"),
        TaskStatus::Blocked,
        "an open dependency keeps it out of the ready queue"
    );
}

#[test]
fn transitions_refuse_the_cases_that_would_lose_information() {
    let tmp = tempdir().expect("tempdir");
    let mut store = fixture(tmp.path());
    let fresh = task(&mut store, "Never parked");
    let parked = task(&mut store, "Parked once");
    park_task(&mut store, parked.id, None).expect("park");

    // Unparking something that was never parked is a mistake to report, not to reinterpret.
    assert!(matches!(
        unpark_task(&mut store, fresh.id),
        Err(StorypoleError::IllegalTransition { .. })
    ));
    // Parking twice is a no-op, so a script can be re-run.
    assert_eq!(
        park_task(&mut store, parked.id, None).expect("idempotent"),
        TaskStatus::Deferred
    );
    // Reopening something that was never verified is likewise refused.
    assert!(matches!(
        reopen_task(&mut store, fresh.id, "because"),
        Err(StorypoleError::IllegalTransition { .. })
    ));
}
