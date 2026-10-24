//! Verification certification and downstream dependency unblocking.

use crate::dag::is_task_dependency_ready;
use crate::error::StorypoleError;
use crate::git::{
    resolve_commit_shas_between, resolve_current_branch, resolve_head_commit, resolve_merge_base,
};
use crate::index::query::{get_all_tasks, get_task_by_id};
use crate::model::LedgerEvent;
use crate::model::decision::VerificationReceipt;
use crate::model::task::{EdgeKind, TaskStatus};
use crate::store::StorypoleStore;
use std::process::Command;
use uuid::Uuid;

/// Verify task execution, certify with a receipt, and unblock dependents.
pub fn verify_task(
    store: &mut StorypoleStore,
    task_id: Uuid,
    receipt_override: Option<VerificationReceipt>,
    run_command: bool,
) -> Result<VerificationReceipt, StorypoleError> {
    let task = get_task_by_id(store.conn(), task_id)?;
    let now = StorypoleStore::now_ms();

    let git_branch = resolve_current_branch(Some(store.root_dir()));
    let head_commit_sha = resolve_head_commit(Some(store.root_dir()));
    let base_commit_sha = task
        .active_claim
        .as_ref()
        .and_then(|c| c.base_commit_sha.clone())
        .or_else(|| resolve_merge_base(Some(store.root_dir()), "main", "HEAD"))
        .or_else(|| head_commit_sha.clone());
    let commit_shas = match (&base_commit_sha, &head_commit_sha) {
        (Some(base), Some(head)) => resolve_commit_shas_between(Some(store.root_dir()), base, head),
        (None, Some(head)) => vec![head.clone()],
        _ => Vec::new(),
    };

    let mut receipt = if let Some(r) = receipt_override {
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
            git_branch: None,
            base_commit_sha: None,
            head_commit_sha: None,
            commit_shas: Vec::new(),
        }
    } else {
        VerificationReceipt {
            verifier: "manual".to_string(),
            command: "n/a".to_string(),
            exit_code: Some(0),
            diagnostic_count: None,
            summary: "Manual verification confirmed".to_string(),
            verified_at_epoch_ms: now,
            git_branch: None,
            base_commit_sha: None,
            head_commit_sha: None,
            commit_shas: Vec::new(),
        }
    };

    if receipt.git_branch.is_none() {
        receipt.git_branch = git_branch;
    }
    if receipt.base_commit_sha.is_none() {
        receipt.base_commit_sha = base_commit_sha;
    }
    if receipt.head_commit_sha.is_none() {
        receipt.head_commit_sha = head_commit_sha;
    }
    if receipt.commit_shas.is_empty() {
        receipt.commit_shas = commit_shas;
    }

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
