//! Deliberate status transitions: parking, unparking and reopening (`SP-47`).
//!
//! `Deferred` was reachable only at creation - `ops/graph.rs` maps a `defer` of a non-bug kind
//! straight to it and nothing else ever wrote that status. Consequences measured on the live board:
//! `GB-155` carried an owner do-not-start directive while reading `Ready`, and `storypole next` kept
//! proposing it as the next task; a `Verified` task could not be reopened (`GB-341`).
//!
//! Each verb therefore has a rule about what it may hide. None of them deletes anything: the ledger
//! keeps the receipt, the claim and every earlier status, so the record can always say how a task got
//! where it is.

use crate::dag::is_task_dependency_ready;
use crate::error::StorypoleError;
use crate::index::query::{get_all_tasks, get_task_by_id};
use crate::model::LedgerEvent;
use crate::model::task::{EdgeKind, TaskStatus};
use crate::store::StorypoleStore;
use uuid::Uuid;

/// Refuses a transition, naming both ends and the reason.
fn refuse(task: &crate::model::TaskNode, to: TaskStatus, why: &str) -> StorypoleError {
    StorypoleError::IllegalTransition {
        ticket: task.ticket_key.clone(),
        from: task.status,
        to,
        why: why.to_string(),
    }
}

/// Record one status change.
fn move_to(
    store: &mut StorypoleStore,
    task: &crate::model::TaskNode,
    to: TaskStatus,
    reason: String,
) -> Result<TaskStatus, StorypoleError> {
    store.record_event(&LedgerEvent::StatusChanged {
        task_id: task.id,
        from: task.status,
        to,
        reason: Some(reason),
        timestamp_ms: StorypoleStore::now_ms(),
    })?;
    Ok(to)
}

/// Parks a task: it stays in the backlog and leaves the ready queue (`SP-47`).
///
/// Parking is deliberately not a way to hide finished work. A `Verified` task is refused because its
/// status is the evidence that the work happened, and `reopen` is the verb that says otherwise
/// honestly. A task under an active claim is refused for the same reason in the other direction:
/// somebody is working on it, and moving it out from under them is how two agents disagree about one
/// ticket.
///
/// # Errors
///
/// [`StorypoleError::IllegalTransition`] when the current status cannot be parked, or when the task is
/// archived, which is already out of the backlog.
pub fn park_task(
    store: &mut StorypoleStore,
    task_id: Uuid,
    reason: Option<&str>,
) -> Result<TaskStatus, StorypoleError> {
    let task = get_task_by_id(store.conn(), task_id)?;
    if task.archived_at_epoch_ms.is_some() {
        return Err(refuse(
            &task,
            TaskStatus::Deferred,
            "an archived task is already out of the backlog, and archive records the status it held",
        ));
    }
    match task.status {
        // Idempotent: parking a parked task is a no-op rather than an error, because a script that
        // parks a set of tickets should not fail on the second run.
        TaskStatus::Deferred => return Ok(TaskStatus::Deferred),
        TaskStatus::Ready | TaskStatus::Triage | TaskStatus::Blocked | TaskStatus::Failed => {}
        TaskStatus::Verified => {
            return Err(refuse(
                &task,
                TaskStatus::Deferred,
                "verified work is finished; reopen it first if it is not, so the reopening is recorded",
            ));
        }
        TaskStatus::InProgress | TaskStatus::Synthesizing | TaskStatus::Verifying => {
            return Err(refuse(
                &task,
                TaskStatus::Deferred,
                "a claim is held and work is in flight; hand off or release the lease first",
            ));
        }
        TaskStatus::Archived => {
            return Err(refuse(
                &task,
                TaskStatus::Deferred,
                "archived is a state rather than a status, and unarchive is the way back",
            ));
        }
    }

    let reason = reason.map_or_else(|| "Parked".to_string(), |why| format!("Parked: {why}"));
    move_to(store, &task, TaskStatus::Deferred, reason)
}

/// Returns a parked task to the backlog (`SP-47`).
///
/// It returns to `Blocked` rather than `Ready` when a dependency is still open: `Ready` is a claim that
/// the task can be started, and a queue that offers work whose prerequisites are unmet is the
/// lying-`Ready` problem `GB-155` demonstrated.
///
/// # Errors
///
/// [`StorypoleError::IllegalTransition`] when the task is not deferred.
pub fn unpark_task(
    store: &mut StorypoleStore,
    task_id: Uuid,
) -> Result<TaskStatus, StorypoleError> {
    let task = get_task_by_id(store.conn(), task_id)?;
    if task.status != TaskStatus::Deferred {
        return Err(refuse(
            &task,
            TaskStatus::Ready,
            "only a deferred task can be returned to the backlog",
        ));
    }

    let all_tasks = get_all_tasks(store.conn())?;
    let open_dependency = task
        .edges
        .iter()
        .filter(|edge| edge.kind == EdgeKind::BlockedBy)
        .any(|edge| {
            all_tasks
                .get(&edge.target_id)
                .is_none_or(|target| target.status != TaskStatus::Verified)
        });
    let to = if open_dependency || !is_task_dependency_ready(&task, &all_tasks) {
        TaskStatus::Blocked
    } else {
        TaskStatus::Ready
    };

    move_to(store, &task, to, "Returned to the backlog".to_string())
}

/// Reopens a verified task, keeping the receipt that certified it (`SP-47`, `GB-341`).
///
/// The receipt is not cleared: replay sets it from the verification event and a status change does not
/// touch it, so the record still shows what was certified and when. A later verification replaces it,
/// which is the difference between "this was wrong" and "this was right once".
///
/// # Errors
///
/// [`StorypoleError::IllegalTransition`] when the task is not verified, or is archived.
pub fn reopen_task(
    store: &mut StorypoleStore,
    task_id: Uuid,
    reason: &str,
) -> Result<TaskStatus, StorypoleError> {
    let task = get_task_by_id(store.conn(), task_id)?;
    if task.archived_at_epoch_ms.is_some() {
        return Err(refuse(
            &task,
            TaskStatus::Ready,
            "unarchive it first; an archived task is out of the backlog",
        ));
    }
    if task.status != TaskStatus::Verified {
        return Err(refuse(
            &task,
            TaskStatus::Ready,
            "only a verified task can be reopened",
        ));
    }

    move_to(
        store,
        &task,
        TaskStatus::Ready,
        format!("Reopened: {reason}"),
    )
}
