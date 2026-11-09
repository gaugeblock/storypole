//! Archive state and expired-claim reaping (`GB-257`, `GB-258`).
//!
//! `GB-258`: archiving records a *state*, never a status - the task keeps the status it earned, and the
//! ledger event says so. `GB-257`: a claim is a lease with a TTL, so a task whose holder stopped
//! heartbeating must come back to the backlog instead of showing as work nobody is doing.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use storypole_core::config::StorypoleConfig;
use storypole_core::index::query::get_task_by_id;
use storypole_core::model::decision::AgentClaim;
use storypole_core::model::events::LedgerEvent;
use storypole_core::model::task::TaskStatus;
use storypole_core::ops::archive::{archive_task, unarchive_task};
use storypole_core::ops::lease::{expired_claims, reap_expired_claims};
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::store::StorypoleStore;
use tempfile::TempDir;
use uuid::Uuid;

fn fresh_store() -> (TempDir, StorypoleStore) {
    let dir = TempDir::new().expect("tempdir");
    fs::create_dir_all(dir.path().join(".storypole")).expect("store dir");
    let store = StorypoleStore::init(dir.path(), StorypoleConfig::default()).expect("init");
    (dir, store)
}

fn new_task(store: &mut StorypoleStore) -> Uuid {
    add_task(
        store,
        CreateTaskParams {
            title: "A task".into(),
            ..Default::default()
        },
    )
    .expect("add task")
    .id
}

fn ledger_text(dir: &TempDir) -> String {
    fs::read_to_string(
        dir.path()
            .join(".gaugeblock")
            .join("storypole")
            .join("ledger.jsonl"),
    )
    .expect("ledger")
}

/// Puts a task `InProgress` under a lease that expired at `expires_at_epoch_ms`.
fn hold_expired_claim(store: &mut StorypoleStore, task_id: Uuid, agent: &str, expires_at: u64) {
    let now = StorypoleStore::now_ms();
    store
        .record_event(&LedgerEvent::StatusChanged {
            task_id,
            from: TaskStatus::Ready,
            to: TaskStatus::InProgress,
            reason: None,
            timestamp_ms: now,
        })
        .expect("in progress");
    store
        .record_event(&LedgerEvent::TaskClaimed {
            task_id,
            claim: AgentClaim {
                agent_id: agent.to_string(),
                claimed_at_epoch_ms: 1,
                lease_expires_at_epoch_ms: expires_at,
                git_branch: None,
                base_commit_sha: None,
            },
        })
        .expect("claim");
}

#[test]
fn archiving_keeps_the_status_and_records_only_the_state() {
    let (dir, mut store) = fresh_store();
    let id = new_task(&mut store);
    let earned = get_task_by_id(store.conn(), id).expect("task").status;

    archive_task(&mut store, id).expect("archive");

    let archived = get_task_by_id(store.conn(), id).expect("task");
    assert_eq!(
        archived.status, earned,
        "archiving must not change the status the task earned"
    );
    assert!(
        archived.archived_at_epoch_ms.is_some(),
        "archived is a state, recorded as a timestamp"
    );

    let ledger = ledger_text(&dir);
    assert!(
        ledger.contains("\"event\":\"task_archived\""),
        "archive must record a state transition"
    );
    assert!(
        !ledger.contains("\"to\":\"archived\""),
        "archive must never claim a status transition to a status that is not one"
    );
}

#[test]
fn unarchiving_clears_the_state_and_keeps_the_status() {
    let (dir, mut store) = fresh_store();
    let id = new_task(&mut store);
    let earned = get_task_by_id(store.conn(), id).expect("task").status;

    archive_task(&mut store, id).expect("archive");
    unarchive_task(&mut store, id).expect("unarchive");

    let restored = get_task_by_id(store.conn(), id).expect("task");
    assert!(restored.archived_at_epoch_ms.is_none(), "state cleared");
    assert_eq!(restored.status, earned, "the status never moved");

    let ledger = ledger_text(&dir);
    assert!(ledger.contains("\"event\":\"task_unarchived\""));
}

#[test]
fn an_expired_claim_is_reported_and_reaped_back_to_ready() {
    let (_dir, mut store) = fresh_store();
    let id = new_task(&mut store);
    hold_expired_claim(&mut store, id, "ghost", 1);

    let expired = expired_claims(&store).expect("scan");
    assert_eq!(expired.len(), 1, "the expired lease must be visible");
    assert_eq!(expired[0].agent_id, "ghost");

    let reaped = reap_expired_claims(&mut store).expect("reap");
    assert_eq!(reaped.len(), 1);
    assert_eq!(
        get_task_by_id(store.conn(), id).expect("task").status,
        TaskStatus::Ready,
        "an expired claim returns the task to the backlog"
    );
    assert!(
        expired_claims(&store).expect("rescan").is_empty(),
        "reaping must be idempotent"
    );
}

#[test]
fn a_live_claim_is_left_alone() {
    let (_dir, mut store) = fresh_store();
    let id = new_task(&mut store);
    let live = StorypoleStore::now_ms() + 3_600_000;
    hold_expired_claim(&mut store, id, "worker", live);

    assert!(
        expired_claims(&store).expect("scan").is_empty(),
        "a lease in the future is not expired"
    );
    assert!(reap_expired_claims(&mut store).expect("reap").is_empty());
    assert_eq!(
        get_task_by_id(store.conn(), id).expect("task").status,
        TaskStatus::InProgress,
        "live work must not be taken away from its holder"
    );
}
