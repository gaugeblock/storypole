//! A plan step can be marked complete (`GB-184`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use storypole_core::config::StorypoleConfig;
use storypole_core::index::query::get_task_by_id;
use storypole_core::model::TaskScope;
use storypole_core::model::plan::{PlanStep, TaskPlan};
use storypole_core::model::task::TaskKind;
use storypole_core::ops::graph::defer_issue;
use storypole_core::ops::plan_ops::{set_plan_step_complete, set_task_plan};
use storypole_core::store::StorypoleStore;

fn step(id: &str) -> PlanStep {
    PlanStep {
        step_id: id.to_string(),
        title: format!("step {id}"),
        target_files: Vec::new(),
        verification_gate: None,
        completed: false,
    }
}

#[test]
fn a_plan_step_can_be_completed_by_position_and_reopened_by_id() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut store = StorypoleStore::init(dir.path(), StorypoleConfig::default()).expect("init");

    let task = defer_issue(
        &mut store,
        "planned work".to_string(),
        "body".to_string(),
        TaskKind::Feature,
        TaskScope::default(),
        None,
        "test".to_string(),
    )
    .expect("defer");

    set_task_plan(
        &mut store,
        task.id,
        TaskPlan {
            summary: "two steps".to_string(),
            design_markdown: "# plan".to_string(),
            steps: vec![step("s1"), step("s2")],
            plan_file_path: None,
            updated_at_epoch_ms: 0,
        },
    )
    .expect("set plan");

    // By one-based position, so a caller need not invent ids.
    let plan = set_plan_step_complete(&mut store, task.id, "1", true).expect("complete");
    assert!(plan.steps[0].completed, "first step should be complete");
    assert!(!plan.steps[1].completed, "second step must be untouched");

    // The change is persisted, not just returned.
    let reloaded = get_task_by_id(store.conn(), task.id).expect("reload");
    let persisted = reloaded.plan.expect("plan persisted");
    assert!(persisted.steps[0].completed);

    // And it can be undone by id.
    let reopened = set_plan_step_complete(&mut store, task.id, "s1", false).expect("undo");
    assert!(!reopened.steps[0].completed, "undo should reopen the step");
}

#[test]
fn an_unknown_step_is_reported_rather_than_ignored() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut store = StorypoleStore::init(dir.path(), StorypoleConfig::default()).expect("init");
    let task = defer_issue(
        &mut store,
        "no such step".to_string(),
        "body".to_string(),
        TaskKind::Bug,
        TaskScope::default(),
        None,
        "test".to_string(),
    )
    .expect("defer");

    set_task_plan(
        &mut store,
        task.id,
        TaskPlan {
            summary: "one step".to_string(),
            design_markdown: "# plan".to_string(),
            steps: vec![step("s1")],
            plan_file_path: None,
            updated_at_epoch_ms: 0,
        },
    )
    .expect("set plan");

    let error = set_plan_step_complete(&mut store, task.id, "s9", true)
        .expect_err("an unknown step must not be silently ignored");
    assert!(error.to_string().contains("s9"), "{error}");
}
