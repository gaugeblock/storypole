//! A refused step reads as what it is (`GB-242`).
//!
//! The plan-step commands raised `InvalidIdentifier` when the step did not exist, so a correct message came
//! out under a wrong prefix: "Invalid task identifier: 'no plan step `99` on SP-1 (7 step(s) recorded)'".
//! Nothing about the identifier was invalid, and a reader chasing that prefix looks in the wrong place.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use storypole_core::StorypoleConfig;
use storypole_core::error::StorypoleError;
use storypole_core::model::plan::{PlanStep, TaskPlan};
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::ops::plan_ops::{set_plan_step_complete, set_task_plan};
use storypole_core::store::StorypoleStore;
use tempfile::tempdir;

fn store_with_plan(root: &Path) -> (StorypoleStore, uuid::Uuid) {
    fs::create_dir_all(root.join(".gaugeblock").join("storypole").join("tickets")).expect("store");
    let mut store = StorypoleStore::init(root, StorypoleConfig::default()).expect("init");
    fs::remove_dir_all(root.join(".storypole")).ok();
    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Has a plan".into(),
            ..Default::default()
        },
    )
    .expect("task");
    let plan = TaskPlan {
        summary: "two steps".into(),
        design_markdown: String::new(),
        steps: vec![
            step("first", "do the first thing"),
            step("second", "do the second"),
        ],
        plan_file_path: None,
        updated_at_epoch_ms: 0,
    };
    set_task_plan(&mut store, task.id, plan).expect("plan");
    (store, task.id)
}

#[test]
fn a_missing_step_is_rejected_without_blaming_the_identifier() {
    let tmp = tempdir().expect("tempdir");
    let (mut store, task) = store_with_plan(tmp.path());

    let error = set_plan_step_complete(&mut store, task, "99", true).expect_err("no such step");
    let rendered = error.to_string();

    assert!(
        !rendered.starts_with("Invalid task identifier"),
        "the prefix contradicted the message: {rendered}"
    );
    assert!(
        rendered.contains("no plan step `99`") && rendered.contains("2 step(s) recorded"),
        "the message still says exactly what was wrong: {rendered}"
    );
    assert!(matches!(error, StorypoleError::Rejected(_)));
}

#[test]
fn a_task_without_a_plan_says_so_plainly() {
    let tmp = tempdir().expect("tempdir");
    fs::create_dir_all(
        tmp.path()
            .join(".gaugeblock")
            .join("storypole")
            .join("tickets"),
    )
    .expect("store");
    let mut store = StorypoleStore::init(tmp.path(), StorypoleConfig::default()).expect("init");
    fs::remove_dir_all(tmp.path().join(".storypole")).ok();
    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "No plan".into(),
            ..Default::default()
        },
    )
    .expect("task");

    let error = set_plan_step_complete(&mut store, task.id, "first", true)
        .expect_err("there is no plan to complete a step of");
    let rendered = error.to_string();
    assert!(
        rendered.contains("has no attached plan"),
        "the refusal names the real problem: {rendered}"
    );
    assert!(
        !rendered.starts_with("Invalid task identifier"),
        "{rendered}"
    );
}

/// One unfinished step with the fields the contract requires.
fn step(id: &str, title: &str) -> PlanStep {
    PlanStep {
        step_id: id.to_string(),
        title: title.to_string(),
        target_files: Vec::new(),
        verification_gate: None,
        completed: false,
    }
}
