//! Operations for setting, updating, and reading task plans.

use crate::error::StorypoleError;
use crate::index::query::get_task_by_id;
use crate::model::LedgerEvent;
use crate::model::plan::{PlanStep, TaskPlan};
use crate::store::StorypoleStore;
use std::fs;
use uuid::Uuid;

/// How a `plan set` treats the plan that is already attached (`GB-321`).
///
/// The distinction is about *content*, never about *history*: a step's completion is evidence that
/// something happened, so it survives every update regardless of mode. Dropping a completed step is
/// therefore its own decision, taken explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlanUpdate {
    /// Steps the new plan does not mention are kept. The default, because the common case is adding a
    /// step or correcting a summary, not restating the whole plan.
    #[default]
    Merge,
    /// The new plan is the whole plan. Steps it does not mention are dropped, unless they are complete -
    /// that needs `force`.
    Replace,
}

/// Replaces or merges a task's plan (`GB-321`).
///
/// # Errors
///
/// [`StorypoleError::PlanStepLoss`] when the update would drop a completed step and `force` is false.
pub fn set_task_plan_with(
    store: &mut StorypoleStore,
    task_id: Uuid,
    mut plan: TaskPlan,
    update: PlanUpdate,
    force: bool,
) -> Result<TaskPlan, StorypoleError> {
    let task = get_task_by_id(store.conn(), task_id)?;
    if let Some(existing) = task.plan.as_ref() {
        plan.steps = reconcile_steps(&task.ticket_key, &existing.steps, plan.steps, update, force)?;
    }
    set_task_plan(store, task_id, plan)
}

/// Combines the recorded steps with the incoming ones, keeping completion and refusing silent loss.
fn reconcile_steps(
    ticket: &str,
    existing: &[PlanStep],
    mut incoming: Vec<PlanStep>,
    update: PlanUpdate,
    force: bool,
) -> Result<Vec<PlanStep>, StorypoleError> {
    // Completion is carried forward for every step that survives, in both modes: an update describes
    // what the plan *is*, not what has been achieved.
    for step in &mut incoming {
        if let Some(previous) = existing.iter().find(|old| old.step_id == step.step_id) {
            step.completed = previous.completed;
        }
    }

    if update == PlanUpdate::Merge {
        // Steps the new plan does not mention are kept, so a partial update cannot quietly erase the
        // rest of the plan - which is exactly what happened when `plan set` replaced wholesale.
        for old in existing {
            if !incoming.iter().any(|new| new.step_id == old.step_id) {
                incoming.push(old.clone());
            }
        }
    }

    // Loss is measured against the *result*, not the input. Measuring it against the input refused a
    // merge that was about to keep every step, because the union had not run yet - caught by running
    // the new refusal against a real six-step plan, not by its tests (`GB-321`).
    let dropped: Vec<String> = existing
        .iter()
        .filter(|old| !incoming.iter().any(|new| new.step_id == old.step_id))
        .filter(|old| old.completed)
        .map(|old| old.step_id.clone())
        .collect();
    if !dropped.is_empty() && !force {
        return Err(StorypoleError::PlanStepLoss {
            ticket: ticket.to_string(),
            steps: dropped,
        });
    }

    Ok(incoming)
}

/// The next unfinished step, in plan order (`GB-321`).
///
/// `None` means every step is complete, which is a statement about the plan rather than an absence.
#[must_use]
pub fn next_step(plan: &TaskPlan) -> Option<&PlanStep> {
    plan.steps.iter().find(|step| !step.completed)
}

/// How much of a plan is done, as `(completed, total)`.
#[must_use]
pub fn plan_progress(plan: &TaskPlan) -> (usize, usize) {
    let done = plan.steps.iter().filter(|step| step.completed).count();
    (done, plan.steps.len())
}

/// Attaches or updates a task's plan, replacing whatever was recorded.
///
/// Prefer [`set_task_plan_with`]: this is the wholesale write it delegates to, and calling it directly
/// discards step completion (`GB-321`).
///
/// # Errors
///
/// [`StorypoleError`] when the task is unknown or the plan file cannot be written.
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
