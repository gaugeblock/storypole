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

    plan.plan_file_path = Some(format!(".storypole/plans/{plan_filename}"));

    store.record_event(&LedgerEvent::PlanUpdated {
        task_id,
        plan: plan.clone(),
        timestamp_ms: now,
    })?;

    Ok(plan)
}
