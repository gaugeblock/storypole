//! Verification certification and downstream dependency unblocking.

mod capture_trace;
mod receipts;
mod target;

use self::capture_trace::capture_trace;
use self::target::{resolve_target_dir, run_miter_verify};
use crate::dag::is_task_dependency_ready;
use crate::error::StorypoleError;
use crate::git::{resolve_current_branch, resolve_head_commit};
use crate::index::query::{get_all_tasks, get_task_by_id};
use crate::model::LedgerEvent;
use crate::model::decision::VerificationReceipt;
use crate::model::task::{EdgeKind, TaskStatus};
use crate::store::StorypoleStore;
use uuid::Uuid;

/// Verify task execution, certify with a receipt, and unblock dependents.
pub fn verify_task(
    store: &mut StorypoleStore,
    task_id: Uuid,
    receipt_override: Option<VerificationReceipt>,
    run_command: bool,
    run_compiler: bool,
) -> Result<VerificationReceipt, StorypoleError> {
    let task = get_task_by_id(store.conn(), task_id)?;

    // Refuse to certify a tree the ticket does not claim (`GB-316`). A task that names a member's files
    // but carries no scope would otherwise be verified against the harness and receive a well-formed,
    // exit-zero receipt about the wrong repository - which is how four tickets were certified before
    // this existed (`GB-313`). Refusing is visible; certifying the wrong tree is not.
    let detected = crate::ops::workflow::detect_scope(store, &task);
    if task.scope.repo.is_none()
        && let Some(candidate) = detected.repo
    {
        return Err(StorypoleError::UnscopedMemberWork {
            ticket: task.ticket_key.clone(),
            repo: candidate,
            derived_from: detected.source.as_str().to_string(),
            harness: store.root_dir().file_name().map_or_else(
                || "the harness root".to_string(),
                |n| n.to_string_lossy().into(),
            ),
        });
    }

    let now = StorypoleStore::now_ms();
    let target_dir = resolve_target_dir(store.root_dir(), &task);

    let git_branch = resolve_current_branch(Some(&target_dir));
    let head_commit_sha = resolve_head_commit(Some(&target_dir));
    let (base_commit_sha, commit_shas) = crate::ops::revision::branch_delta(
        &target_dir,
        task.active_claim
            .as_ref()
            .and_then(|c| c.base_commit_sha.as_deref()),
        head_commit_sha.as_deref(),
    );

    let diagnostic_count = if run_compiler && receipt_override.is_none() {
        Some(run_miter_verify(&target_dir)?)
    } else {
        None
    };

    let (mut receipt, chain_used) = receipts::build(receipts::BuildRequest {
        store,
        task: &task,
        target_dir: &target_dir,
        now,
        diagnostic_count,
        receipt_override,
        run_command,
        run_compiler,
    })?;

    // Which repository and which chain this receipt describes (`GB-316`). `repo` comes from the task's
    // placement, so a member ticket's receipt can never name the harness; `chain_key` is the
    // configuration entry that actually ran, because falling back to `default` was otherwise invisible.
    if receipt.repo.is_none() {
        receipt.repo = task.scope.repo.clone();
    }
    if receipt.chain_key.is_none() {
        receipt.chain_key = chain_used;
    }

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
    // A receipt covering one commit because no base could be named says so, rather than letting the
    // count read as a small change (`GB-326`).
    if receipt.base_commit_sha.is_none() && !receipt.commit_shas.is_empty() {
        receipt.summary = format!(
            "{}\n{}",
            crate::ops::revision::unbounded_window_note(),
            receipt.summary
        );
    }

    store.record_event(&LedgerEvent::TaskVerified {
        task_id,
        receipt: receipt.clone(),
    })?;

    // Capture one Silver trace for this verified head (GB-165). Verification already happened, so a
    // spool failure is reported and swallowed rather than failing the transition.
    if let Err(error) = capture_trace(store, &task, &receipt) {
        tracing::warn!(ticket = %task.ticket_key, %error, "trace capture failed");
    }

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
