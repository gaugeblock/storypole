//! Task archiving and restoration operations.

use crate::error::StorypoleError;
use crate::index::query::get_task_by_id;
use crate::model::LedgerEvent;
use crate::model::task::TaskStatus;
use crate::store::StorypoleStore;
use uuid::Uuid;

/// Archive a completed or closed task, removing it from active backlog queries.
pub fn archive_task(store: &mut StorypoleStore, task_id: Uuid) -> Result<(), StorypoleError> {
    let task = get_task_by_id(store.conn(), task_id)?;
    if task.status == TaskStatus::Archived {
        return Ok(());
    }

    let now = StorypoleStore::now_ms();
    store.record_event(&LedgerEvent::TaskArchived {
        task_id,
        status: task.status,
        timestamp_ms: now,
    })?;

    Ok(())
}

/// Restore an archived task back to active backlog status.
pub fn unarchive_task(
    store: &mut StorypoleStore,
    task_id: Uuid,
) -> Result<TaskStatus, StorypoleError> {
    let task = get_task_by_id(store.conn(), task_id)?;
    // Archived is a state, not a status (GB-179), so the guard asks whether the task is archived
    // rather than whether its status reads `Archived` - which it never will again.
    if task.archived_at_epoch_ms.is_none() {
        return Ok(task.status);
    }

    let target_status = if task.verification_receipt.is_some() {
        TaskStatus::Verified
    } else {
        TaskStatus::Ready
    };

    let now = StorypoleStore::now_ms();
    store.record_event(&LedgerEvent::TaskUnarchived {
        task_id,
        timestamp_ms: now,
    })?;

    Ok(target_status)
}
