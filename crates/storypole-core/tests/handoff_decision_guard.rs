//! Regression tests for GB-146: a handoff packet must not clear a pending-decision block.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use storypole_core::config::StorypoleConfig;
use storypole_core::index::query::get_task_by_id;
use storypole_core::model::decision::HandoffPacket;
use storypole_core::model::task::{TaskScope, TaskStatus};
use storypole_core::ops::decisions::{ask_decision, decide};
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::ops::workflow::{claim_task, handoff_task};
use storypole_core::store::StorypoleStore;
use uuid::Uuid;

fn temp_store() -> StorypoleStore {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = StorypoleConfig {
        claim_ttl_minutes: 60,
        ..Default::default()
    };
    StorypoleStore::init(tmp.path(), config).expect("init")
}

fn handoff_packet(blocker: &str) -> HandoffPacket {
    HandoffPacket {
        recorded_by_agent: "agent-1".into(),
        timestamp_ms: StorypoleStore::now_ms(),
        completed_items: vec!["scaffolded the module".into()],
        remaining_items: vec!["apply the architecture call".into()],
        last_error_or_blocker: Some(blocker.to_string()),
        git_ref: None,
        dirty_files: vec![],
    }
}

fn task_blocked_on_decision(store: &mut StorypoleStore) -> (Uuid, Uuid) {
    let task = add_task(
        store,
        CreateTaskParams {
            title: "Handoff guard subject".into(),
            scope: TaskScope {
                repo: Some("gaugeblock".into()),
                crate_name: Some("storypole-core".into()),
                target_files: vec!["crates/storypole-core/src/ops/workflow.rs".into()],
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("task");
    claim_task(store, task.id, "agent-1", None).expect("claim");
    let decision = ask_decision(
        store,
        task.id,
        "architecture".into(),
        "Coerce or reject a ready handoff while a decision is pending?".into(),
        vec!["coerce to blocked".into(), "reject the handoff".into()],
        None,
    )
    .expect("decision");
    let blocked = get_task_by_id(store.conn(), task.id).expect("reload");
    assert_eq!(
        blocked.status,
        TaskStatus::Blocked,
        "asking blocks the task"
    );
    (task.id, decision)
}

#[test]
fn handoff_cannot_clear_a_pending_decision_block() {
    let mut store = temp_store();
    let (task_id, _) = task_blocked_on_decision(&mut store);

    let effective = handoff_task(
        &mut store,
        task_id,
        handoff_packet("decision pending"),
        TaskStatus::Ready,
    )
    .expect("handoff");

    assert_eq!(
        effective,
        TaskStatus::Blocked,
        "requesting ready must not win over an unanswered decision"
    );
    let task = get_task_by_id(store.conn(), task_id).expect("reload");
    assert_eq!(task.status, TaskStatus::Blocked);
    assert!(
        task.latest_handoff.is_some(),
        "the packet is still recorded so remaining work survives the block"
    );
    assert!(
        task.active_claim.is_none(),
        "handing off still releases the lease"
    );
}

#[test]
fn handoff_reaches_ready_once_the_decision_is_resolved() {
    let mut store = temp_store();
    let (task_id, decision) = task_blocked_on_decision(&mut store);

    decide(
        &mut store,
        task_id,
        decision,
        "coerce to blocked".into(),
        "A pending decision is an explicit block.".into(),
        "architect".into(),
        None,
        None,
    )
    .expect("decide");

    let effective = handoff_task(
        &mut store,
        task_id,
        handoff_packet("none"),
        TaskStatus::Ready,
    )
    .expect("handoff");

    assert_eq!(effective, TaskStatus::Ready);
    assert_eq!(
        get_task_by_id(store.conn(), task_id)
            .expect("reload")
            .status,
        TaskStatus::Ready
    );
}

#[test]
fn handoff_can_still_report_failure_while_a_decision_is_pending() {
    let mut store = temp_store();
    let (task_id, _) = task_blocked_on_decision(&mut store);

    let effective = handoff_task(
        &mut store,
        task_id,
        handoff_packet("toolchain broken"),
        TaskStatus::Failed,
    )
    .expect("handoff");

    assert_eq!(
        effective,
        TaskStatus::Failed,
        "a failure is not claimable work, so it needs no downgrade"
    );
}
