//! Workflow execution lifecycle: claims, heartbeats, handoffs, and task selection.

use crate::dag::briefing::build_task_briefing;
use crate::dag::is_task_dependency_ready;
use crate::error::StorypoleError;
use crate::git::{checkout_or_create_branch, resolve_current_branch, resolve_head_commit};
use crate::index::query::{TaskFilter, get_all_tasks, get_task_by_id, query_tasks};
use crate::model::LedgerEvent;
use crate::model::decision::AgentClaim;
use crate::model::events::TaskContextBriefing;
use crate::model::task::TaskStatus;
pub use crate::ops::handoff::{handoff_task, heartbeat};
pub use crate::ops::verification::verify_task;
use crate::store::StorypoleStore;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::ops::scope::{ResolvedScope, resolve_scope};

/// What counts as a member for storypole's purposes (`GB-189`).
///
/// A checkout is a member, and so is a directory that only holds a manifest: storypole is usable
/// standalone, where the repositories of a workspace are not necessarily git clones. The shared
/// topology takes the rule as a parameter precisely so each consumer can state its own.
fn storypole_member(path: &std::path::Path) -> bool {
    path.join(".git").exists() || path.join("Cargo.toml").exists()
}

/// Places a task in a member repository, persisting the derivation (`GB-316`).
///
/// A member store owns its tasks outright, so there is nothing to place. A workspace store owns no
/// code: each task belongs to the member its scope names, and a task that names none is placed here
/// or is honestly harness-level work.
///
/// The derivation is written once, as a `TaskUpdated` event, so later commands can report how the
/// placement was made and cannot silently disagree with it.
pub(crate) fn place_task(
    store: &mut StorypoleStore,
    task: crate::model::TaskNode,
) -> Result<crate::model::TaskNode, StorypoleError> {
    if store.root_dir().join(".storypole").is_dir() {
        return Ok(task);
    }
    let members: Vec<String> =
        gaugeblock_workspace::WorkspaceTopology::discover_with(store.root_dir(), storypole_member)
            .members()
            .iter()
            .map(|member| member.name.clone())
            .collect();
    let caller_dir = std::env::current_dir().ok();
    let resolved: ResolvedScope = resolve_scope(
        None,
        task.scope.repo.as_deref(),
        &task.scope.target_files,
        &task.scope.target_symbols,
        &members,
        caller_dir.as_deref(),
        store.root_dir(),
    );

    let Some(repo) = resolved.repo else {
        return Ok(task);
    };
    if task.scope.repo.as_deref() == Some(repo.as_str()) {
        return Ok(task);
    }

    let mut scope = task.scope.clone();
    scope.repo = Some(repo.clone());
    let placed = crate::ops::mutations::update_task(
        store,
        task.id,
        crate::ops::mutations::UpdateTaskParams {
            scope: Some(scope),
            ..Default::default()
        },
    )?;
    tracing::info!(
        task = %task.ticket_key,
        repo = %repo,
        source = resolved.source.as_str(),
        "placed task in a repository"
    );
    Ok(placed)
}

/// Resolves the git repository that owns a task, if any.
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

    // Placement happens before the lease, because the lease records the branch and base commit of the
    // repository that owns the work - and until the scope is set, nothing knows which repository that
    // is. Deriving it here and persisting it means `verify` reads a decision instead of making one
    // (`GB-316`).
    let task = place_task(store, task)?;

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
