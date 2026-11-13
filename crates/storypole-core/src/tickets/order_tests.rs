//! Ordering against a handoff clock (`SP-54`).
//!
//! `HandoffRecorded` used to fall through to the `0` arm of the timestamp table, so a handoff sorted
//! ahead of every real event no matter when it happened. A legacy ledger replays linearly, so the two
//! orders disagreed and `migrate` refused the store; worse, the written record reported `InProgress`
//! for a ticket that had been handed back to `ready`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use uuid::Uuid;

use super::*;
use crate::model::LedgerEvent;
use gaugeblock_contracts::storypole::{HandoffPacket, TaskStatus};

/// A status change at `at`.
fn moved(id: Uuid, to: TaskStatus, at: u64) -> LedgerEvent {
    LedgerEvent::StatusChanged {
        task_id: id,
        from: TaskStatus::Ready,
        to,
        reason: None,
        timestamp_ms: at,
    }
}

/// A handoff recorded at `at`, as `handoff_task` builds it.
fn handed_off(id: Uuid, to: TaskStatus, at: u64) -> LedgerEvent {
    LedgerEvent::HandoffRecorded {
        task_id: id,
        handoff: HandoffPacket {
            recorded_by_agent: "cline".to_string(),
            timestamp_ms: at,
            completed_items: Vec::new(),
            remaining_items: Vec::new(),
            last_error_or_blocker: None,
            git_ref: None,
            dirty_files: Vec::new(),
        },
        new_status: to,
    }
}

#[test]
fn a_handoff_sorts_by_its_own_clock_not_at_zero() {
    let id = Uuid::from_u128(4);
    // The recorded order a real ledger holds: a claim, then a handoff 216 seconds later.
    let mut events = vec![
        moved(id, TaskStatus::InProgress, 200),
        handed_off(id, TaskStatus::Ready, 260),
    ];

    order_events(&mut events);

    let stamps: Vec<u64> = events.iter().map(event_timestamp).collect();
    assert_eq!(
        stamps,
        vec![200, 260],
        "the handoff carries its own clock instead of sorting at zero"
    );
    assert!(
        matches!(events.last(), Some(LedgerEvent::HandoffRecorded { .. })),
        "replay ends on the handoff, so a handed-off ticket reads ready"
    );
}
