//! Lease maintenance and handoff: heartbeats and structured continuation packets.
//!
//! Split out of `ops::workflow` (GB-156) so claiming and scheduling stay readable apart from the
//! continuity primitives an agent uses while it holds a task.

use crate::error::StorypoleError;
use crate::index::query::get_task_by_id;
use crate::model::LedgerEvent;
use crate::model::decision::{AgentClaim, DecisionStatus, HandoffPacket};
use crate::model::task::TaskStatus;
use crate::store::StorypoleStore;
use uuid::Uuid;

/// Renew lease time-to-live for an active agent claim.
pub fn heartbeat(
    store: &mut StorypoleStore,
    task_id: Uuid,
    agent_id: &str,
) -> Result<u64, StorypoleError> {
    let task = get_task_by_id(store.conn(), task_id)?;
    let now = StorypoleStore::now_ms();

    let claim = match task.active_claim {
        Some(ref c) if c.agent_id == agent_id && !c.is_expired(now) => c.clone(),
        _ => return Err(StorypoleError::ClaimExpired(task_id)),
    };

    let ttl_ms = (store.config().claim_ttl_minutes as u64) * 60 * 1000;
    let new_claim = AgentClaim {
        agent_id: claim.agent_id,
        claimed_at_epoch_ms: claim.claimed_at_epoch_ms,
        lease_expires_at_epoch_ms: now + ttl_ms,
        git_branch: claim.git_branch,
        base_commit_sha: claim.base_commit_sha,
    };

    store.record_event(&LedgerEvent::TaskClaimed {
        task_id,
        claim: new_claim.clone(),
    })?;

    Ok(new_claim.lease_expires_at_epoch_ms)
}

/// Record structured handoff packet and transition task to Ready, Blocked, or Failed.
///
/// Returns the status the task actually moved to. A pending decision is an explicit block that a
/// handoff must not clear, so a requested [`TaskStatus::Ready`] is downgraded to
/// [`TaskStatus::Blocked`] until the decision is resolved - the same rule
/// [`crate::dag::is_task_dependency_ready`] enforces at claim time. The packet itself is always
/// recorded, so the remaining items survive the block.
pub fn handoff_task(
    store: &mut StorypoleStore,
    task_id: Uuid,
    packet: HandoffPacket,
    new_status: TaskStatus,
) -> Result<TaskStatus, StorypoleError> {
    let task = get_task_by_id(store.conn(), task_id)?;
    let decision_pending = task
        .decisions
        .iter()
        .any(|decision| decision.status == DecisionStatus::Pending);
    let effective_status = if new_status == TaskStatus::Ready && decision_pending {
        TaskStatus::Blocked
    } else {
        new_status
    };

    store.record_event(&LedgerEvent::HandoffRecorded {
        task_id,
        handoff: packet,
        new_status: effective_status,
    })?;
    Ok(effective_status)
}
