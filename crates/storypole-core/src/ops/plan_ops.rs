//! Operations for setting, updating, and reading task plans.

use crate::error::StorypoleError;
use crate::index::query::get_task_by_id;
use crate::model::LedgerEvent;
use crate::model::plan::TaskPlan;
use crate::store::StorypoleStore;
use std::fs;
use uuid::Uuid;

/// Attach or update a TaskPlan on a task, persisting markdown to `.storypole/plans/<KEY>.md`.
pub fn set_task_plan(
    store: &mut StorypoleStore,
    task_id: Uuid,
    mut plan: TaskPlan,
) -> Result<TaskPlan, StorypoleError> {
    let task = get_task_by_id(store.conn(), task_id)?;
    let now = StorypoleStore::now_ms();
    plan.updated_at_epoch_ms = now;

    let plans_dir = store.dot_dir().join("plans");
    fs::create_dir_all(&plans_dir).map_err(|e| StorypoleError::IoWithPath {
        path: plans_dir.clone(),
        source: e,
    })?;

    let plan_filename = format!("{}.md", task.ticket_key);
    let plan_file = plans_dir.join(&plan_filename);
    fs::write(&plan_file, &plan.design_markdown).map_err(|e| StorypoleError::IoWithPath {
        path: plan_file.clone(),
        source: e,
    })?;

    let rel_prefix = if store.dot_dir().to_string_lossy().contains(".gaugeblock") {
        ".gaugeblock/storypole"
    } else {
        ".storypole"
    };
    plan.plan_file_path = Some(format!("{rel_prefix}/plans/{plan_filename}"));

    store.record_event(&LedgerEvent::PlanUpdated {
        task_id,
        plan: plan.clone(),
        timestamp_ms: now,
    })?;

    Ok(plan)
}

/// Marks one plan step complete, or reopens it.
///
/// `PlanStep::completed` has always existed; nothing could set it, so a plan recorded intent and never
/// progress (`GB-184`). The step is matched by id, falling back to a one-based position so a caller can
/// tick steps without inventing ids.
///
/// # Errors
///
/// [`StorypoleError::NotFound`]-style errors when the task has no plan or the step does not exist.
pub fn set_plan_step_complete(
    store: &mut StorypoleStore,
    task_id: Uuid,
    step: &str,
    completed: bool,
) -> Result<TaskPlan, StorypoleError> {
    let task = get_task_by_id(store.conn(), task_id)?;
    let Some(mut plan) = task.plan.clone() else {
        return Err(StorypoleError::InvalidIdentifier(format!(
            "task {} has no attached plan",
            task.ticket_key
        )));
    };

    let by_id = plan.steps.iter().position(|s| s.step_id == step);
    let by_index = step
        .parse::<usize>()
        .ok()
        .and_then(|n| n.checked_sub(1))
        .filter(|n| *n < plan.steps.len());
    let Some(index) = by_id.or(by_index) else {
        return Err(StorypoleError::InvalidIdentifier(format!(
            "no plan step `{step}` on {} ({} step(s) recorded)",
            task.ticket_key,
            plan.steps.len()
        )));
    };

    plan.steps[index].completed = completed;
    set_task_plan(store, task_id, plan)
}
