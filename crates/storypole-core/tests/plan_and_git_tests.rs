#![allow(clippy::unwrap_used, clippy::expect_used)]

use storypole_core::model::plan::{PlanStep, TaskPlan};
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::ops::plan_ops::set_task_plan;
use storypole_core::ops::verification::verify_task;
use storypole_core::ops::workflow::claim_task;
use storypole_core::{StorypoleConfig, StorypoleStore};
use tempfile::TempDir;

#[test]
fn test_set_task_plan_persistence() {
    let tmp = TempDir::new().expect("tempdir");
    let mut store = StorypoleStore::init(tmp.path(), StorypoleConfig::default()).expect("init");

    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Task with Plan".to_string(),
            description: "Test plan attachment".to_string(),
            ..Default::default()
        },
    )
    .expect("add_task");

    let plan = TaskPlan {
        summary: "Plan summary".to_string(),
        design_markdown: "# Architectural Blueprint\n\nStep details here.".to_string(),
        steps: vec![
            PlanStep {
                step_id: "S1".to_string(),
                title: "First Step".to_string(),
                target_files: vec!["src/lib.rs".to_string()],
                verification_gate: Some("cargo test".to_string()),
                completed: false,
            },
            PlanStep {
                step_id: "S2".to_string(),
                title: "Second Step".to_string(),
                target_files: vec!["src/main.rs".to_string()],
                verification_gate: None,
                completed: false,
            },
        ],
        plan_file_path: None,
        updated_at_epoch_ms: 0,
    };

    let updated_plan = set_task_plan(&mut store, task.id, plan).expect("set_task_plan");
    assert_eq!(
        updated_plan.plan_file_path,
        Some(format!(".storypole/plans/{}.md", task.ticket_key))
    );

    let plan_file_on_disk = tmp
        .path()
        .join(".storypole")
        .join("plans")
        .join(format!("{}.md", task.ticket_key));
    assert!(
        plan_file_on_disk.exists(),
        "Plan file must be created on disk"
    );
    let content = std::fs::read_to_string(&plan_file_on_disk).expect("read");
    assert!(content.contains("# Architectural Blueprint"));

    // Verify reloaded task from index has the plan
    let reloaded = storypole_core::index::query::get_task_by_id(store.conn(), task.id)
        .expect("get_task_by_id");
    assert!(reloaded.plan.is_some());
    let attached = reloaded.plan.unwrap();
    assert_eq!(attached.summary, "Plan summary");
    assert_eq!(attached.steps.len(), 2);
    assert_eq!(attached.steps[0].step_id, "S1");
}

#[test]
fn test_claim_and_verify_git_tracking() {
    let tmp = TempDir::new().expect("tempdir");
    let mut store = StorypoleStore::init(tmp.path(), StorypoleConfig::default()).expect("init");

    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Task with Git Verification".to_string(),
            ..Default::default()
        },
    )
    .expect("add_task");

    let briefing =
        claim_task(&mut store, task.id, "agent-git", Some("feat/my-branch")).expect("claim_task");
    assert!(briefing.task.active_claim.is_some());
    let claim = briefing.task.active_claim.as_ref().unwrap();
    assert_eq!(claim.agent_id, "agent-git");

    let receipt = verify_task(&mut store, task.id, None, false, false).expect("verify_task");
    assert_eq!(receipt.verifier, "manual");
}
