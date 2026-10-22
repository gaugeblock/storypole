//! Integration tests for DAG cycle detection and chained decision lifecycles.

use storypole_core::config::StorypoleConfig;
use storypole_core::model::decision::DecisionStatus;
use storypole_core::model::task::{EdgeKind, TaskStatus};
use storypole_core::ops::decisions::{ask_decision, decide, supersede_decision};
use storypole_core::ops::graph::link_tasks;
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::store::StorypoleStore;

fn temp_store() -> StorypoleStore {
    let tmp = tempfile::tempdir().expect("tempdir");
    StorypoleStore::init(tmp.path(), StorypoleConfig::default()).expect("init")
}

#[test]
fn test_cycle_rejection_on_blocked_by() {
    let mut store = temp_store();

    let task_a = add_task(
        &mut store,
        CreateTaskParams {
            title: "Task A".into(),
            ..Default::default()
        },
    )
    .expect("add A");

    let task_b = add_task(
        &mut store,
        CreateTaskParams {
            title: "Task B".into(),
            ..Default::default()
        },
    )
    .expect("add B");

    let task_c = add_task(
        &mut store,
        CreateTaskParams {
            title: "Task C".into(),
            ..Default::default()
        },
    )
    .expect("add C");

    // A is blocked by B; B is blocked by C
    link_tasks(&mut store, task_a.id, task_b.id, EdgeKind::BlockedBy).expect("link A->B");
    link_tasks(&mut store, task_b.id, task_c.id, EdgeKind::BlockedBy).expect("link B->C");

    // Attempting C blocked by A creates cycle (A -> B -> C -> A)
    let cycle_err = link_tasks(&mut store, task_c.id, task_a.id, EdgeKind::BlockedBy);
    assert!(cycle_err.is_err(), "Expected cycle detection error");
}

#[test]
fn test_self_cycle_rejection() {
    let mut store = temp_store();
    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Self Task".into(),
            ..Default::default()
        },
    )
    .expect("add");

    let err = link_tasks(&mut store, task.id, task.id, EdgeKind::BlockedBy);
    assert!(err.is_err());
}

#[test]
fn test_chained_decisions_and_auto_unblock() {
    let mut store = temp_store();

    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Decision Task".into(),
            ..Default::default()
        },
    )
    .expect("add");

    assert_eq!(task.status, TaskStatus::Ready);

    // 1. Raise decision D1 -> Task transitions to Blocked
    let d1_id = ask_decision(
        &mut store,
        task.id,
        "architecture".into(),
        "Which parser engine?".into(),
        vec!["nom".into(), "winnow".into()],
        None,
    )
    .expect("ask d1");

    let t_blocked =
        storypole_core::index::query::get_task_by_id(store.conn(), task.id).expect("get");
    assert_eq!(t_blocked.status, TaskStatus::Blocked);

    // 2. Resolve D1 and spawn D2 -> Task remains Blocked
    let d2_id = uuid::Uuid::new_v4();
    let d2 = storypole_core::model::decision::TaskDecision {
        decision_id: d2_id,
        parent_decision_id: Some(d1_id),
        status: DecisionStatus::Pending,
        category: "architecture".into(),
        question: "Use SIMD acceleration?".into(),
        proposed_options: vec!["yes".into(), "no".into()],
        chosen_option: None,
        rationale: None,
        resolved_by: None,
        updated_at_epoch_ms: 0,
    };

    decide(
        &mut store,
        task.id,
        d1_id,
        "winnow".into(),
        "Faster compile time".into(),
        "colin".into(),
        Some(d2),
        Some("Must use winnow 0.7+".into()),
    )
    .expect("decide d1");

    let t_still_blocked =
        storypole_core::index::query::get_task_by_id(store.conn(), task.id).expect("get");
    assert_eq!(t_still_blocked.status, TaskStatus::Blocked);
    assert!(
        t_still_blocked
            .acceptance_criteria
            .contains(&"Must use winnow 0.7+".to_string())
    );

    // 3. Resolve D2 -> Task transitions to Ready automatically!
    decide(
        &mut store,
        task.id,
        d2_id,
        "no".into(),
        "Simpler portability".into(),
        "colin".into(),
        None,
        None,
    )
    .expect("decide d2");

    let t_unblocked =
        storypole_core::index::query::get_task_by_id(store.conn(), task.id).expect("get");
    assert_eq!(t_unblocked.status, TaskStatus::Ready);
}

#[test]
fn test_decision_supersede_cascades() {
    let mut store = temp_store();

    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Supersede Task".into(),
            ..Default::default()
        },
    )
    .expect("add");

    let d1_id = ask_decision(
        &mut store,
        task.id,
        "api".into(),
        "Transport?".into(),
        vec!["grpc".into(), "jsonrpc".into()],
        None,
    )
    .expect("d1");

    let d2_id = uuid::Uuid::new_v4();
    let d2 = storypole_core::model::decision::TaskDecision {
        decision_id: d2_id,
        parent_decision_id: Some(d1_id),
        status: DecisionStatus::Pending,
        category: "api".into(),
        question: "Encoding?".into(),
        proposed_options: vec!["json".into(), "cbor".into()],
        chosen_option: None,
        rationale: None,
        resolved_by: None,
        updated_at_epoch_ms: 0,
    };

    decide(
        &mut store,
        task.id,
        d1_id,
        "jsonrpc".into(),
        "Simpler stdio".into(),
        "agent".into(),
        Some(d2),
        None,
    )
    .expect("decide d1");

    // Superseding D1 should mark both D1 and D2 as Superseded, and block the task
    supersede_decision(
        &mut store,
        task.id,
        d1_id,
        "gRPC needed for streaming".into(),
    )
    .expect("supersede");

    let t = storypole_core::index::query::get_task_by_id(store.conn(), task.id).expect("get");
    assert_eq!(t.status, TaskStatus::Blocked);
    for d in t.decisions {
        if d.decision_id == d1_id || d.decision_id == d2_id {
            assert_eq!(d.status, DecisionStatus::Superseded);
        }
    }
}
