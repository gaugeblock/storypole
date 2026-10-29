//! Workflow execution lifecycle: claims, heartbeats, handoffs, and task selection.

use crate::dag::briefing::build_task_briefing;
use crate::dag::is_task_dependency_ready;
use crate::error::StorypoleError;
use crate::git::{checkout_or_create_branch, resolve_current_branch, resolve_head_commit};
use crate::index::query::{TaskFilter, get_all_tasks, get_task_by_id, query_tasks};
use crate::model::LedgerEvent;
use crate::model::decision::{AgentClaim, DecisionStatus, HandoffPacket};
use crate::model::events::TaskContextBriefing;
use crate::model::task::TaskStatus;
pub use crate::ops::verification::verify_task;
use crate::store::StorypoleStore;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Resolves the git repository that owns a task, if any.
///
/// A *member store* keeps `.storypole` at the repository root and therefore owns every task in it.
/// A *workspace store* keeps `.gaugeblock/storypole` at the meta-harness root, which owns no code:
/// each task belongs to the member named by `scope.repo`, and a task without a repo scope belongs
/// to no repository at all.
///
/// Returns `None` when no repository owns the task, in which case callers must not touch branches.
pub fn resolve_task_repo(store_root: &Path, task: &crate::model::TaskNode) -> Option<PathBuf> {
    if store_root.join(".storypole").is_dir() {
        return Some(store_root.to_path_buf());
    }

    let repo = task.scope.repo.as_deref()?;
    let candidate = store_root.join(repo);
    // `.git` is a directory for normal clones and a file for worktrees, so `exists()` covers both.
    if candidate.join(".git").exists() {
        Some(candidate)
    } else {
        None
    }
}

/// Acquire an agent lease claim on a task and transition to `InProgress`.
pub fn claim_task(
    store: &mut StorypoleStore,
    task_id: Uuid,
    agent_id: &str,
    branch_override: Option<&str>,
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

    let task_repo = resolve_task_repo(store.root_dir(), &task);

    if let Some(b) = branch_override {
        match task_repo.as_deref() {
            Some(repo_dir) => {
                if let Err(e) = checkout_or_create_branch(Some(repo_dir), b) {
                    tracing::warn!("Failed to checkout branch '{b}': {e}");
                }
            }
            // No repository owns this task (harness-level ledger): record the intended branch in the
            // lease without moving any checkout.
            None => tracing::debug!(
                "Task has no owning git repository; recording branch '{b}' without checkout"
            ),
        }
    }

    let git_branch = task_repo
        .as_deref()
        .and_then(|dir| resolve_current_branch(Some(dir)))
        .or_else(|| branch_override.map(ToString::to_string));
    let base_commit_sha = task_repo
        .as_deref()
        .and_then(|dir| resolve_head_commit(Some(dir)));

    let ttl_ms = (store.config().claim_ttl_minutes as u64) * 60 * 1000;
    let claim = AgentClaim {
        agent_id: agent_id.to_string(),
        claimed_at_epoch_ms: now,
        lease_expires_at_epoch_ms: now + ttl_ms,
        git_branch,
        base_commit_sha,
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
            let claimed_briefing = claim_task(store, task.id, agent, None)?;
            return Ok(Some(claimed_briefing));
        }

        return Ok(Some(briefing));
    }

    Ok(None)
}
