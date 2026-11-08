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

/// A warning line when a task's receipt names commits that `main` never received.
///
/// Returns `None` when there is nothing to say: no receipt, no recorded head, no repository to ask, or
/// commits that are reachable.
#[must_use]
pub fn unmerged_receipt_warning(store_root: &Path, task: &TaskNode) -> Option<String> {
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

#[cfg(test)]
mod tests;
