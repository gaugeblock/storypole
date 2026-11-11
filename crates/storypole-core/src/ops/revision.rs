//! Whether a receipt's commits are actually reachable from a merged branch (`GB-325`).
//!
//! A receipt is written on the feature branch, before the merge, so at verify time it is always
//! unmerged and saying so would be noise. The question that matters is asked later: has anything come
//! of it? Two tickets were certified with exit-zero receipts while their commits sat on branches that
//! `main` never received - `GB-250` and `GB-302` - and nothing in the ledger disagreed.

use std::path::Path;

use crate::model::TaskNode;

/// Whether `sha` is reachable from `main` in the repository at `repo_dir`.
///
/// `None` when git cannot answer - a missing repository, an unknown revision, or a repository with no
/// `main`. Unknown is not "merged": a check that reports success when it cannot run is the defect this
/// module exists to remove.
#[must_use]
pub fn is_reachable_from_main(repo_dir: &Path, sha: &str) -> Option<bool> {
    let repository = gaugeblock_git::Repository::discover(repo_dir).ok()?;
    repository.is_ancestor(sha, "main").ok()
}

/// The commit window a receipt should record: the branch delta, not a single commit (`GB-326`).
///
/// The previous rule took the claim's base commit when there was one, and the claim's base *is* `HEAD`
/// whenever a task is claimed after its work is already committed - which is what a resumed or
/// re-claimed ticket looks like. The window then resolved empty and the receipt fell back to a single
/// commit, so a receipt for a one-commit change and a receipt for an eleven-commit change could both
/// read `[1 commit(s)]`.
///
/// The branch point is the honest base: the merge-base against `main`, or the claim's base when that is
/// the earlier ancestor. A candidate equal to `HEAD`, or not an ancestor of it, is not a base at all.
#[must_use]
pub fn branch_delta(
    target_dir: &Path,
    claim_base: Option<&str>,
    head: Option<&str>,
) -> (Option<String>, Vec<String>) {
    let Some(head) = head else {
        return (None, Vec::new());
    };
    let mut candidates: Vec<String> = Vec::new();
    if let Some(base) = claim_base {
        candidates.push(base.to_string());
    }
    if let Some(merge_base) = crate::git::resolve_merge_base(Some(target_dir), "main", "HEAD") {
        candidates.push(merge_base);
    }

    let widest = candidates
        .into_iter()
        .filter(|base| base != head)
        .filter(|base| is_ancestor(target_dir, base, head))
        .max_by_key(|base| {
            crate::git::resolve_commit_shas_between(Some(target_dir), base, head).len()
        });

    match widest {
        Some(base) => {
            let shas = crate::git::resolve_commit_shas_between(Some(target_dir), &base, head);
            (Some(base), shas)
        }
        // No base can be named, so the window is `HEAD` alone - and the caller must say so rather than
        // letting one commit stand in for a branch.
        None => (None, vec![head.to_string()]),
    }
}

/// Whether `ancestor` is reachable from `descendant`.
fn is_ancestor(dir: &Path, ancestor: &str, descendant: &str) -> bool {
    gaugeblock_git::Repository::discover(dir)
        .ok()
        .and_then(|repo| repo.is_ancestor(ancestor, descendant).ok())
        .unwrap_or(false)
}

/// A note for the receipt when the commit window could only be narrowed to `HEAD`.
#[must_use]
pub fn unbounded_window_note() -> &'static str {
    "[window: no base commit could be named, so this receipt covers HEAD only]"
}

/// A warning line when a receipt's recorded window no longer exists (`SP-41`).
///
/// The write-time `window_intact` field records what was true when the receipt was written, which is
/// almost always "the commits I just certified exist". The question a reader actually has is asked
/// later, after a rebase, a squash or a force-push, so it is asked here against the repository the
/// receipt names, and answered honestly. `None` means there is nothing to say: no receipt, no window,
/// no repository to ask, or a window still intact.
#[must_use]
pub fn orphaned_window_warning(store_root: &Path, task: &TaskNode) -> Option<String> {
    let receipt = task.verification_receipt.as_ref()?;
    let head = receipt.head_commit_sha.as_deref()?;
    let repo_dir = match receipt.repo.as_deref() {
        Some(name) => store_root.join(name),
        None => store_root.to_path_buf(),
    };
    let base = receipt.base_commit_sha.as_deref();
    if window_exists(&repo_dir, base, Some(head))? {
        return None;
    }
    Some(format!(
        "warning: the window the receipt for {} names ({}) no longer exists in {} - it was rewritten or \
         garbage-collected, so diff_hash {} and the captured diff are the only record of what was \
         certified.",
        task.ticket_key,
        base.map_or_else(
            || head[..head.len().min(9)].to_string(),
            |base| format!(
                "{}..{}",
                &base[..base.len().min(9)],
                &head[..head.len().min(9)]
            )
        ),
        receipt.repo.as_deref().unwrap_or("the harness"),
        receipt.diff_hash.as_deref().unwrap_or("<none recorded>"),
    ))
}

/// A warning line when a task receipt names commits that `main` never received.
///
/// Returns `None` when there is nothing to say: no receipt, no recorded head, no repository to ask, or
/// commits that are reachable.
#[must_use]
pub fn unmerged_receipt_warning(
    store_root: &Path,
    task: &TaskNode,
) -> Option<String> {
    let receipt = task.verification_receipt.as_ref()?;
    let head = receipt.head_commit_sha.as_deref()?;
    let repo = receipt.repo.as_deref();
    let repo_dir = match repo {
        Some(name) => store_root.join(name),
        None => store_root.to_path_buf(),
    };
    match is_reachable_from_main(&repo_dir, head)? {
        true => None,
        false => Some(format!(
            "warning: the receipt for {} names commit {} on {}, which main has not received - the work \
             it certifies exists only on {}. Merge it, or the receipt is the only place it exists.",
            task.ticket_key,
            &head[..head.len().min(9)],
            repo.unwrap_or("the harness"),
            receipt
                .git_branch
                .as_deref()
                .unwrap_or("an unrecorded branch"),
        )),
    }
}

mod change;
pub use change::{CapturedChange, MAX_DIFF_BYTES, capture_change, window_exists};

#[cfg(test)]
mod tests;
