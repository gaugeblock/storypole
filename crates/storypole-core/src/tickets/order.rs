//! Event ordering (`GB-329`).
//!
//! Replay order comes from the record, never from file position: a merge that interleaves two histories
//! must not let a stale status win because its line landed later.

use uuid::Uuid;

use crate::model::LedgerEvent;

/// Orders events so replay does not depend on file order.
///
/// A stable sort on the event's own clock: events describing the same instant keep the order they were
/// recorded in.
pub fn order_events(events: &mut [LedgerEvent]) {
    events.sort_by_key(|event| (event_timestamp(event), task_id_of_event(event)));
}

/// The event's own clock in epoch milliseconds.
///
/// Every mutation variant carries `timestamp_ms`; creation carries the task's creation time and a receipt
/// the time it was verified, so no event sorts at zero by accident.
#[must_use]
pub fn event_timestamp(event: &LedgerEvent) -> u64 {
    match event {
        LedgerEvent::TaskCreated { task } | LedgerEvent::DeferredItemCaptured { task, .. } => {
            task.created_at_epoch_ms
        }
        LedgerEvent::TaskVerified { receipt, .. } => receipt.verified_at_epoch_ms,
        LedgerEvent::TaskClaimed { claim, .. } => claim.claimed_at_epoch_ms,
        other => timestamp_of(other),
    }
}

/// `timestamp_ms` for every variant that spells one.
fn timestamp_of(event: &LedgerEvent) -> u64 {
    match event {
        LedgerEvent::TaskUpdated { timestamp_ms, .. }
        | LedgerEvent::StatusChanged { timestamp_ms, .. }
        | LedgerEvent::EdgeAdded { timestamp_ms, .. }
        | LedgerEvent::EdgeRemoved { timestamp_ms, .. }
        | LedgerEvent::DecisionRaised { timestamp_ms, .. }
        | LedgerEvent::DecisionResolved { timestamp_ms, .. }
        | LedgerEvent::DecisionSuperseded { timestamp_ms, .. }
        | LedgerEvent::TaskArchived { timestamp_ms, .. }
        | LedgerEvent::TaskUnarchived { timestamp_ms, .. }
        | LedgerEvent::TaskReleased { timestamp_ms, .. }
        | LedgerEvent::TaskSplit { timestamp_ms, .. }
        | LedgerEvent::PlanUpdated { timestamp_ms, .. } => *timestamp_ms,
        LedgerEvent::TaskCreated { .. }
        | LedgerEvent::DeferredItemCaptured { .. }
        | LedgerEvent::TaskVerified { .. }
        | LedgerEvent::TaskClaimed { .. }
        | LedgerEvent::HandoffRecorded { .. } => 0,
    }
}

/// The task an event belongs to.
///
/// Public because a writer has to resolve which record an event belongs to before it can append (`GB-329`).
#[must_use]
pub fn task_id_of_event(event: &LedgerEvent) -> Uuid {
    match event {
        LedgerEvent::TaskCreated { task } | LedgerEvent::DeferredItemCaptured { task, .. } => {
            task.id
        }
        LedgerEvent::TaskVerified { task_id, .. }
        | LedgerEvent::TaskClaimed { task_id, .. }
        | LedgerEvent::TaskUpdated { task_id, .. }
        | LedgerEvent::StatusChanged { task_id, .. }
        | LedgerEvent::EdgeAdded { task_id, .. }
        | LedgerEvent::EdgeRemoved { task_id, .. }
        | LedgerEvent::DecisionRaised { task_id, .. }
        | LedgerEvent::DecisionResolved { task_id, .. }
        | LedgerEvent::DecisionSuperseded { task_id, .. }
        | LedgerEvent::TaskArchived { task_id, .. }
        | LedgerEvent::TaskUnarchived { task_id, .. }
        | LedgerEvent::TaskReleased { task_id, .. }
        | LedgerEvent::HandoffRecorded { task_id, .. }
        | LedgerEvent::PlanUpdated { task_id, .. } => *task_id,
        LedgerEvent::TaskSplit { parent_task_id, .. } => *parent_task_id,
    }
}
