//! Verification certification and downstream dependency unblocking.

mod capture_trace;
mod children;
mod receipts;
mod refusals;
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

/// What a reader is told when the window a receipt names no longer exists (`SP-41`).
pub const ORPHANED_WINDOW_NOTE: &str = "[window: a commit this receipt names no longer exists, so the \
     diff_hash and diff below are the only record of what was certified]";

/// The repository that owns this task's crate, when the task names only a crate (`SP-50`).
///
/// The chain lookup used to answer "no chain" for a task scoped `crate_name: storypole-cli` with no
/// repository, even though `storypole` declares that crate and configures a full chain. The receipt
/// then certified `cargo test` and could not say which repository it had measured, which is the two
/// halves of the same gap: nothing resolved a crate to its owner.
///
/// Evidence, not a guess: the answer comes from `[package] name` in the member manifests
/// (`migrate::manifests::crate_owners`), which is the same map the ticket migration adopted work by.
/// A crate no member declares returns `None`, and the caller keeps the honest fallback.
fn owning_repo(store: &StorypoleStore, crate_name: &str) -> Option<String> {
    // Canonicalised before asking membership, because `Membership::containing` walks ancestors and a
    // relative path stops at the current directory (the `OF-42` lesson).
    let root = store.root_dir();
    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    // A member's store answers for its suite; a lone checkout answers for itself.
    let suite_root = gaugeblock_workspace::Membership::containing(&canonical)
        .map_or_else(|| canonical.clone(), |membership| membership.suite_root);
    let topology = gaugeblock_workspace::WorkspaceTopology::discover(&suite_root);
    let members: Vec<(String, std::path::PathBuf)> = topology
        .members()
        .iter()
        .map(|member| (member.name.clone(), member.path.clone()))
        .collect();
    crate::migrate::manifests::crate_owners(&members)
        .get(crate_name)
        .cloned()
}

/// Verify task execution, certify with a receipt, and unblock dependents.
///
/// The note-less form of [`verify_task_with_note`], kept so every existing call site does not have to
/// pass a `None` it will never use.
pub fn verify_task(
    store: &mut StorypoleStore,
    task_id: Uuid,
    receipt_override: Option<VerificationReceipt>,
    run_command: bool,
    run_compiler: bool,
) -> Result<VerificationReceipt, StorypoleError> {
    verify_task_with_note(
        store,
        task_id,
        receipt_override,
        run_command,
        run_compiler,
        None,
    )
}

/// Verify task execution, appending `note` to whatever the run proved (`SP-49`).
///
/// The note is the caller's statement of what they checked. It never substitutes for the command, the
/// exit code or the chain key: a receipt whose evidence is a sentence is the failure `GB-272` ended, and
/// `--summary` was reproducing it by building a manual override on the executing path.
pub fn verify_task_with_note(
    store: &mut StorypoleStore,
    task_id: Uuid,
    receipt_override: Option<VerificationReceipt>,
    run_command: bool,
    run_compiler: bool,
    note: Option<&str>,
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

    // The plan is the ticket's own statement of what finishing means (`GB-334`). Verifying past an unfinished
    // plan certifies something other than the ticket - which is how GB-57 closed with 0 of 6 steps.
    refusals::unchecked_plan_steps(&task)?;
    // An epic is a promise that the work is done, so it cannot be certified while a child of it is open
    // (`GB-334`). Children are found through the edges they carry, and only within this store - seeing the rest
    // needs the workspace rollup.
    children::unresolved_children(store.conn(), &task)?;

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

    // Which repository this task belongs to: its own, or the one that declares its crate (`SP-50`).
    // Computed once so the chain lookup and the receipt cannot disagree about it.
    let task_repo = task.scope.repo.clone().or_else(|| {
        task.scope
            .crate_name
            .as_deref()
            .and_then(|c| owning_repo(store, c))
    });

    let (mut receipt, chain_used) = receipts::build(receipts::BuildRequest {
        store,
        task: &task,
        target_dir: &target_dir,
        now,
        diagnostic_count,
        receipt_override,
        run_command,
        run_compiler,
        note,
        repo_for_chain: task_repo.as_deref(),
    })?;

    // Which repository and which chain this receipt describes (`GB-316`). `repo` comes from the task's
    // placement, so a member ticket's receipt can never name the harness; `chain_key` is the
    // configuration entry that actually ran, because falling back to `default` was otherwise invisible.
    if receipt.repo.is_none() {
        receipt.repo = task_repo;
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

    // The change this receipt certifies, captured as content beside the window (`SP-41`). Unconditional:
    // orphaning is discovered later by whoever reads the receipt, and a record that only exists when the
    // author expected trouble is not a record.
    let captured = crate::ops::revision::capture_change(
        &target_dir,
        receipt.base_commit_sha.as_deref(),
        receipt.head_commit_sha.as_deref(),
    );
    if receipt.diff_hash.is_none() {
        receipt.diff_hash = captured.hash;
    }
    if receipt.diff.is_none() {
        receipt.diff = captured.diff;
    }
    if receipt.window_intact.is_none() {
        receipt.window_intact = captured.window_intact;
    }
    // An orphaned window is reported where a reader will see it, not swallowed: the receipt is now the
    // only record of what it certified (`SP-41`).
    if receipt.window_intact == Some(false) {
        receipt.summary = format!("{}\n{}", ORPHANED_WINDOW_NOTE, receipt.summary);
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
