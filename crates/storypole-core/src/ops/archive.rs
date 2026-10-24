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
    store.record_event(&LedgerEvent::StatusChanged {
        task_id,
        from: task.status,
        to: TaskStatus::Archived,
        reason: Some("Archived out of active backlog".to_string()),
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
    if task.status != TaskStatus::Archived {
        return Ok(task.status);
    }

    let target_status = if task.verification_receipt.is_some() {
        TaskStatus::Verified
    } else {
        TaskStatus::Ready
    };

    let now = StorypoleStore::now_ms();
    store.record_event(&LedgerEvent::StatusChanged {
        task_id,
        from: TaskStatus::Archived,
        to: target_status,
        reason: Some("Unarchived back to active backlog".to_string()),
        timestamp_ms: now,
    })?;

    Ok(target_status)
}
