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
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

fn resolve_target_dir(root: &Path, task: &crate::model::TaskNode) -> PathBuf {
    if let Some(repo) = task.scope.repo.as_deref() {
        let candidate = root.join(repo);
        if candidate.is_dir() {
            return candidate;
        }
    }
    if let Some(crate_name) = task.scope.crate_name.as_deref() {
        let base_crate = crate_name.split('-').next().unwrap_or(crate_name);
        let candidate = root.join(base_crate);
        if candidate.is_dir() {
            return candidate;
        }
    }
    root.to_path_buf()
}

fn run_miter_verify(dir: &Path) -> Result<u32, StorypoleError> {
    let output = Command::new("miter")
        .arg("verify")
        .arg("--path")
        .arg(dir)
        .arg("--json")
        .output()
        .map_err(|e| StorypoleError::VerificationFailed(format!("Failed to invoke miter: {e}")))?;

    if !output.status.success() && output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(StorypoleError::VerificationFailed(format!(
            "miter verify failed: {stderr}"
        )));
    }

    let val: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|e| {
        StorypoleError::VerificationFailed(format!("Failed to parse miter JSON output: {e}"))
    })?;

    let outcome = val
        .get("outcome")
        .and_then(|o| o.as_str())
        .unwrap_or("unknown");
    if outcome == "success" {
        Ok(0)
    } else {
        let msg = val
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Compiler diagnostic failure");
        let spans_len = val
            .get("spans")
            .and_then(|s| s.as_array())
            .map(|a| a.len() as u32)
            .unwrap_or(1);
        Err(StorypoleError::VerificationFailed(format!(
            "miter compiler verification failed ({spans_len} diagnostic(s)): {msg}"
        )))
    }
}

/// Verify task execution, certify with a receipt, and unblock dependents.
pub fn verify_task(
    store: &mut StorypoleStore,
    task_id: Uuid,
    receipt_override: Option<VerificationReceipt>,
    run_command: bool,
    run_compiler: bool,
) -> Result<VerificationReceipt, StorypoleError> {
    let task = get_task_by_id(store.conn(), task_id)?;
    let now = StorypoleStore::now_ms();
    let target_dir = resolve_target_dir(store.root_dir(), &task);

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

    let diagnostic_count = if run_compiler && receipt_override.is_none() {
        Some(run_miter_verify(&target_dir)?)
    } else {
        None
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
            .current_dir(&target_dir)
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

        let verifier = if run_compiler {
            "miter+storypole-verify".to_string()
        } else {
            "storypole-verify".to_string()
        };

        VerificationReceipt {
            verifier,
            command: cmd_str.to_string(),
            exit_code,
            diagnostic_count,
            summary,
            verified_at_epoch_ms: now,
            git_branch: None,
            base_commit_sha: None,
            head_commit_sha: None,
            commit_shas: Vec::new(),
        }
    } else if run_compiler {
        VerificationReceipt {
            verifier: "miter-verify".to_string(),
            command: "miter verify".to_string(),
            exit_code: Some(0),
            diagnostic_count,
            summary: "Compiler verification passed with 0 diagnostics".to_string(),
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
