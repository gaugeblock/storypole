//! Workflow execution lifecycle: claims, heartbeats, handoffs, and verification gates.

use crate::dag::briefing::build_task_briefing;
use crate::dag::is_task_dependency_ready;
use crate::error::StorypoleError;
use crate::index::query::{TaskFilter, get_all_tasks, get_task_by_id, query_tasks};
use crate::model::LedgerEvent;
use crate::model::decision::{AgentClaim, HandoffPacket, VerificationReceipt};
use crate::model::events::TaskContextBriefing;
use crate::model::task::{EdgeKind, TaskStatus};
use crate::store::StorypoleStore;
use std::process::Command;
use uuid::Uuid;

/// Acquire an agent lease claim on a task and transition to `InProgress`.
pub fn claim_task(
    store: &mut StorypoleStore,
    task_id: Uuid,
    agent_id: &str,
) -> Result<TaskContextBriefing, StorypoleError> {
    let task = get_task_by_id(store.conn(), task_id)?;
    let now = StorypoleStore::now_ms();

    if let Some(ref claim) = task.active_claim
        && !claim.is_expired(now)
        && claim.agent_id != agent_id
    {
        return Err(StorypoleError::ClaimConflict {
            task_id,
            current_agent: claim.agent_id.clone(),
            expires_at: claim.lease_expires_at_epoch_ms,
        });
    }

    let ttl_ms = (store.config().claim_ttl_minutes as u64) * 60 * 1000;
    let claim = AgentClaim {
        agent_id: agent_id.to_string(),
        claimed_at_epoch_ms: now,
        lease_expires_at_epoch_ms: now + ttl_ms,
    };

    store.record_event(&LedgerEvent::TaskClaimed {
        task_id,
        claim: claim.clone(),
    })?;

    if task.status != TaskStatus::InProgress {
        store.record_event(&LedgerEvent::StatusChanged {
            task_id,
            from: task.status,
            to: TaskStatus::InProgress,
            reason: Some(format!("Claimed by {agent_id}")),
            timestamp_ms: now,
        })?;
    }

    let updated_task = get_task_by_id(store.conn(), task_id)?;
    let all_tasks = get_all_tasks(store.conn())?;
    Ok(build_task_briefing(
        &updated_task,
        &all_tasks,
        store.config(),
        now,
    ))
}

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
    };

    store.record_event(&LedgerEvent::TaskClaimed {
        task_id,
        claim: new_claim.clone(),
    })?;

    Ok(new_claim.lease_expires_at_epoch_ms)
}

/// Record structured handoff packet and transition task to Ready, Blocked, or Failed.
pub fn handoff_task(
    store: &mut StorypoleStore,
    task_id: Uuid,
    packet: HandoffPacket,
    new_status: TaskStatus,
) -> Result<(), StorypoleError> {
    store.record_event(&LedgerEvent::HandoffRecorded {
        task_id,
        handoff: packet,
        new_status,
    })?;
    Ok(())
}

/// Verify task execution, certify with a receipt, and unblock dependents.
pub fn verify_task(
    store: &mut StorypoleStore,
    task_id: Uuid,
    receipt_override: Option<VerificationReceipt>,
    run_command: bool,
) -> Result<VerificationReceipt, StorypoleError> {
    let task = get_task_by_id(store.conn(), task_id)?;
    let now = StorypoleStore::now_ms();

    let receipt = if let Some(r) = receipt_override {
        r
    } else if run_command {
        let cmd_opt = task
            .scope
            .crate_name
            .as_deref()
            .and_then(|c| store.config().verification_command_for(c))
            .or_else(|| {
                task.scope
                    .repo
                    .as_deref()
                    .and_then(|r| store.config().verification_command_for(r))
            });

        let cmd_str = cmd_opt.unwrap_or("cargo test");
        let output = Command::new("sh")
            .arg("-c")
            .arg(cmd_str)
            .output()
            .map_err(|e| StorypoleError::VerificationFailed(e.to_string()))?;

        let exit_code = output.status.code();
        let summary = String::from_utf8_lossy(&output.stdout).to_string();

        if exit_code != Some(0) {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(StorypoleError::VerificationFailed(format!(
                "Command '{cmd_str}' exited with {exit_code:?}: {stderr}"
            )));
        }

        VerificationReceipt {
            verifier: "storypole-verify".to_string(),
            command: cmd_str.to_string(),
            exit_code,
            diagnostic_count: None,
            summary,
            verified_at_epoch_ms: now,
        }
    } else {
        VerificationReceipt {
            verifier: "manual".to_string(),
            command: "n/a".to_string(),
            exit_code: Some(0),
            diagnostic_count: None,
            summary: "Manual verification confirmed".to_string(),
            verified_at_epoch_ms: now,
        }
    };

    store.record_event(&LedgerEvent::TaskVerified {
        task_id,
        receipt: receipt.clone(),
    })?;

    // Unblock any downstream tasks whose dependencies are now all satisfied
    let all_tasks = get_all_tasks(store.conn())?;
    for (id, downstream) in &all_tasks {
        if downstream.status == TaskStatus::Blocked {
            let depends_on_this = downstream
                .edges
                .iter()
                .any(|e| e.kind == EdgeKind::BlockedBy && e.target_id == task_id);
            if depends_on_this && is_task_dependency_ready(downstream, &all_tasks) {
                store.record_event(&LedgerEvent::StatusChanged {
                    task_id: *id,
                    from: TaskStatus::Blocked,
                    to: TaskStatus::Ready,
                    reason: Some(format!("Dependency {} verified", task.ticket_key)),
                    timestamp_ms: now,
                })?;
            }
        }
    }

    Ok(receipt)
}

/// Retrieve the next optimal ready task, respecting priorities and collision constraints.
pub fn next_task(
    store: &mut StorypoleStore,
    filter: &TaskFilter,
    auto_claim_agent: Option<&str>,
) -> Result<Option<TaskContextBriefing>, StorypoleError> {
    let now = StorypoleStore::now_ms();
    let candidates = query_tasks(store.conn(), filter)?;
    let all_tasks = get_all_tasks(store.conn())?;

    for task in candidates {
        let is_ready = task.status == TaskStatus::Ready;
        let is_claim_expired = task.status == TaskStatus::InProgress
            && task
                .active_claim
                .as_ref()
                .map(|c| c.is_expired(now))
                .unwrap_or(true);

        if !is_ready && !is_claim_expired {
            continue;
        }

        if !is_task_dependency_ready(&task, &all_tasks) {
            continue;
        }

        let briefing = build_task_briefing(&task, &all_tasks, store.config(), now);
        if filter.exclude_file_collisions && !briefing.file_collision_warnings.is_empty() {
            continue;
        }

        if let Some(agent) = auto_claim_agent {
            let claimed_briefing = claim_task(store, task.id, agent)?;
            return Ok(Some(claimed_briefing));
        }

        return Ok(Some(briefing));
    }

    Ok(None)
}
