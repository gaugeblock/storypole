//! Plans carry their progress, and an update cannot lose it by accident (`GB-321`).
//!
//! `plan set` replaced the whole plan. Passing only `--file` and `--summary` dropped six recorded steps
//! and reported success - observed live on `GB-316`, which is why the merge is now the default and a
//! completed step is only dropped when the caller asks for it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use storypole_core::StorypoleConfig;
use storypole_core::model::plan::{PlanStep, TaskPlan};
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::ops::plan_ops::{
    PlanUpdate, next_step, plan_progress, set_plan_step_complete, set_task_plan_with,
};
use storypole_core::store::StorypoleStore;
use tempfile::tempdir;

fn store_at(root: &Path) -> StorypoleStore {
    StorypoleStore::init(root, StorypoleConfig::default()).expect("init")
}

fn task(store: &mut StorypoleStore) -> uuid::Uuid {
    add_task(
        store,
        CreateTaskParams {
            title: "Planned work".into(),
            ..Default::default()
        },
    )
    .expect("add task")
    .id
}

fn step(id: &str) -> PlanStep {
    PlanStep {
        step_id: id.to_string(),
        title: format!("{id} title"),
        target_files: Vec::new(),
        verification_gate: Some(format!("{id} gate")),
        completed: false,
    }
}

fn plan(steps: Vec<PlanStep>) -> TaskPlan {
    TaskPlan {
        summary: "Plan".into(),
        design_markdown: "# Plan\n".into(),
        steps,
        plan_file_path: None,
        updated_at_epoch_ms: 0,
    }
}

#[test]
fn a_partial_update_keeps_the_steps_it_does_not_mention() {
    let tmp = tempdir().expect("tempdir");
    let mut store = store_at(tmp.path());
    let id = task(&mut store);
    set_task_plan_with(
        &mut store,
        id,
        plan(vec![step("one"), step("two"), step("three")]),
        PlanUpdate::Merge,
        false,
    )
    .expect("attach");

    // Only "two" is restated - the update that used to wipe the plan.
    let updated = set_task_plan_with(
        &mut store,
        id,
        plan(vec![step("two")]),
        PlanUpdate::Merge,
        false,
    )
    .expect("merge");

    let ids: Vec<&str> = updated.steps.iter().map(|s| s.step_id.as_str()).collect();
    assert_eq!(ids, vec!["two", "one", "three"], "no step was lost");
}

#[test]
fn completion_survives_an_update_that_restates_the_step() {
    let tmp = tempdir().expect("tempdir");
    let mut store = store_at(tmp.path());
    let id = task(&mut store);
    set_task_plan_with(
        &mut store,
        id,
        plan(vec![step("one"), step("two")]),
        PlanUpdate::Merge,
        false,
    )
    .expect("attach");
    set_plan_step_complete(&mut store, id, "one", true).expect("complete one");

    let updated = set_task_plan_with(
        &mut store,
        id,
        plan(vec![step("one"), step("two")]),
        PlanUpdate::Merge,
        false,
    )
    .expect("update");

    assert!(
        updated
            .steps
            .iter()
            .any(|s| s.step_id == "one" && s.completed),
        "the update described the plan without undoing what was achieved"
    );
}

#[test]
fn replace_drops_an_incomplete_step_but_refuses_a_complete_one() {
    let tmp = tempdir().expect("tempdir");
    let mut store = store_at(tmp.path());
    let id = task(&mut store);
    set_task_plan_with(
        &mut store,
        id,
        plan(vec![step("done"), step("todo")]),
        PlanUpdate::Merge,
        false,
    )
    .expect("attach");
    set_plan_step_complete(&mut store, id, "done", true).expect("complete");

    let refusal = set_task_plan_with(
        &mut store,
        id,
        plan(vec![step("todo")]),
        PlanUpdate::Replace,
        false,
    )
    .expect_err("dropping a completed step must be refused");

    match refusal {
        storypole_core::error::StorypoleError::PlanStepLoss { steps, .. } => {
            assert_eq!(
                steps,
                vec!["done".to_string()],
                "the refusal names the step"
            );
        }
        other => panic!("expected a step-loss refusal, got {other:?}"),
    }

    // Asked for, it happens.
    let replaced = set_task_plan_with(
        &mut store,
        id,
        plan(vec![step("todo")]),
        PlanUpdate::Replace,
        true,
    )
    .expect("forced replace");
    let ids: Vec<&str> = replaced.steps.iter().map(|s| s.step_id.as_str()).collect();
    assert_eq!(ids, vec!["todo"]);
}

#[test]
fn the_next_step_is_the_first_unfinished_one() {
    let tmp = tempdir().expect("tempdir");
    let mut store = store_at(tmp.path());
    let id = task(&mut store);
    set_task_plan_with(
        &mut store,
        id,
        plan(vec![step("one"), step("two")]),
        PlanUpdate::Merge,
        false,
    )
    .expect("attach");
    set_plan_step_complete(&mut store, id, "one", true).expect("complete");

    let task = storypole_core::index::query::get_task_by_id(store.conn(), id).expect("read");
    let plan = task.plan.as_ref().expect("plan attached");

    assert_eq!(next_step(plan).map(|s| s.step_id.as_str()), Some("two"));
    assert_eq!(plan_progress(plan), (1, 2));
}

#[test]
fn a_finished_plan_reports_that_rather_than_nothing() {
    let tmp = tempdir().expect("tempdir");
    let mut store = store_at(tmp.path());
    let id = task(&mut store);
    set_task_plan_with(
        &mut store,
        id,
        plan(vec![step("only")]),
        PlanUpdate::Merge,
        false,
    )
    .expect("attach");
    set_plan_step_complete(&mut store, id, "only", true).expect("complete");

    let task = storypole_core::index::query::get_task_by_id(store.conn(), id).expect("read");
    let plan = task.plan.as_ref().expect("plan attached");

    assert!(next_step(plan).is_none(), "every step is done");
    assert_eq!(plan_progress(plan), (1, 1));
}
