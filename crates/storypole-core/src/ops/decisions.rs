//! Architectural decision lifecycle operations: ask, decide, and supersede.

use crate::dag::is_task_dependency_ready;
use crate::error::StorypoleError;
use crate::index::query::{get_all_tasks, get_task_by_id};
use crate::model::LedgerEvent;
use crate::model::decision::{DecisionStatus, TaskDecision};
use crate::model::task::TaskStatus;
use crate::store::StorypoleStore;
use uuid::Uuid;

/// Raise a pending decision on a task, automatically moving task to `Blocked`.
pub fn ask_decision(
    store: &mut StorypoleStore,
    task_id: Uuid,
    category: String,
    question: String,
    proposed_options: Vec<String>,
    parent_decision_id: Option<Uuid>,
) -> Result<Uuid, StorypoleError> {
    let task = get_task_by_id(store.conn(), task_id)?;
    let decision_id = Uuid::new_v4();
    let now = StorypoleStore::now_ms();

    let decision = TaskDecision {
        decision_id,
        parent_decision_id,
        status: DecisionStatus::Pending,
        category,
        question,
        proposed_options,
        chosen_option: None,
        rationale: None,
        resolved_by: None,
        updated_at_epoch_ms: now,
    };

    store.record_event(&LedgerEvent::DecisionRaised {
        task_id,
        decision,
        timestamp_ms: now,
    })?;

    if task.status != TaskStatus::Blocked {
        store.record_event(&LedgerEvent::StatusChanged {
            task_id,
            from: task.status,
            to: TaskStatus::Blocked,
            reason: Some("Pending architectural decision required".to_string()),
            timestamp_ms: now,
        })?;
    }

    Ok(decision_id)
}

/// Resolve a pending decision, optionally spawning a follow-up or appending an acceptance criterion.
#[allow(clippy::too_many_arguments)]
pub fn decide(
    store: &mut StorypoleStore,
    task_id: Uuid,
    decision_id: Uuid,
    chosen_option: String,
    rationale: String,
    resolved_by: String,
    spawned_followup: Option<TaskDecision>,
    appended_criterion: Option<String>,
) -> Result<(), StorypoleError> {
    let task = get_task_by_id(store.conn(), task_id)?;
    if !task.decisions.iter().any(|d| d.decision_id == decision_id) {
        return Err(StorypoleError::DecisionNotFound(decision_id));
    }

    let now = StorypoleStore::now_ms();
    store.record_event(&LedgerEvent::DecisionResolved {
        task_id,
        decision_id,
        chosen_option,
        rationale,
        resolved_by,
        spawned_followup,
        appended_criterion,
        timestamp_ms: now,
    })?;

    // Check if task can be unblocked to Ready
    let updated_task = get_task_by_id(store.conn(), task_id)?;
    let all_tasks = get_all_tasks(store.conn())?;
    if updated_task.status == TaskStatus::Blocked
        && is_task_dependency_ready(&updated_task, &all_tasks)
    {
        store.record_event(&LedgerEvent::StatusChanged {
            task_id,
            from: TaskStatus::Blocked,
            to: TaskStatus::Ready,
            reason: Some("All pending decisions and dependencies satisfied".to_string()),
            timestamp_ms: now,
        })?;
    }

    Ok(())
}

/// Supersede a previously resolved decision and return task to `Blocked`.
pub fn supersede_decision(
    store: &mut StorypoleStore,
    task_id: Uuid,
    decision_id: Uuid,
    reason: String,
) -> Result<(), StorypoleError> {
    let task = get_task_by_id(store.conn(), task_id)?;
    if !task.decisions.iter().any(|d| d.decision_id == decision_id) {
        return Err(StorypoleError::DecisionNotFound(decision_id));
    }

    let now = StorypoleStore::now_ms();
    store.record_event(&LedgerEvent::DecisionSuperseded {
        task_id,
        decision_id,
        reason: reason.clone(),
        timestamp_ms: now,
    })?;

    if task.status != TaskStatus::Blocked {
        store.record_event(&LedgerEvent::StatusChanged {
            task_id,
            from: task.status,
            to: TaskStatus::Blocked,
            reason: Some(format!("Decision superseded: {reason}")),
            timestamp_ms: now,
        })?;
    }

    Ok(())
}
