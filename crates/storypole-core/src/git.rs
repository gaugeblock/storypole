//! Git provenance: the branch, the commit range and the merge base a verification receipt records.
//!
//! The process spawning lives in `gaugeblock-git` (`GB-131` step 6). What stays here is storypole's
//! policy: every function takes an `Option<&Path>` and degrades to `None` or to the head sha rather
//! than failing, because a receipt with no git context is still a valid receipt and a task can be
//! tracked outside a repository.

use std::path::Path;

use gaugeblock_git::Repository;

/// Opens the repository at `dir`, or the current directory when absent.
fn open(dir: Option<&Path>) -> Option<Repository> {
    Repository::discover(dir.unwrap_or_else(|| Path::new("."))).ok()
}

/// Resolve the current branch name, if any.
#[must_use]
pub fn resolve_current_branch(dir: Option<&Path>) -> Option<String> {
    // A detached HEAD is deliberately `None`: a receipt recording "HEAD" as a branch name would read
    // as a branch that does not exist.
    open(dir)?.current_branch().ok().flatten()
}

/// Resolve the current HEAD commit sha, if any.
#[must_use]
pub fn resolve_head_commit(dir: Option<&Path>) -> Option<String> {
    open(dir)?.head_sha().ok()
}

/// Check out `branch`, creating it if it does not exist yet.
///
/// # Errors
///
/// The git error text when neither path works - an invalid name, or a checkout refused because the
/// working tree would be overwritten.
pub fn checkout_or_create_branch(dir: Option<&Path>, branch: &str) -> Result<String, String> {
    let Some(repo) = open(dir) else {
        return Err("not a git repository".to_string());
    };
    if repo.checkout(branch).is_ok() {
        return Ok(branch.to_string());
    }
    repo.checkout_new_branch(branch)
        .map(|()| branch.to_string())
        .map_err(|error| error.to_string())
}

/// Resolve the merge base between a base branch (e.g. `main`) and `head`, if any.
#[must_use]
pub fn resolve_merge_base(dir: Option<&Path>, base_branch: &str, head: &str) -> Option<String> {
    let repo = open(dir)?;
    if let Ok(base) = repo.merge_base(base_branch, head)
        && !base.is_empty()
    {
        return Some(base);
    }
    // A local `main` that has never been fetched is common in a fresh clone; the remote-tracking ref
    // is the fallback that makes the range correct there.
    let remote_base = format!("origin/{base_branch}");
    repo.merge_base(&remote_base, head)
        .ok()
        .filter(|base| !base.is_empty())
}

/// Resolve the commit shas in the delta between `base_sha` and `head_sha`, oldest first.
///
/// Oldest first is deliberate - a receipt lists commits in the order they were made - and the
/// underlying `rev-list` answers newest first, so the list is reversed here.
#[must_use]
pub fn resolve_commit_shas_between(
    dir: Option<&Path>,
    base_sha: &str,
    head_sha: &str,
) -> Vec<String> {
    if base_sha == head_sha {
        return vec![head_sha.to_string()];
    }
    let Some(repo) = open(dir) else {
        return vec![head_sha.to_string()];
    };
    let range = format!("{base_sha}..{head_sha}");
    match repo.commits_between(&range) {
        Ok(mut commits) if !commits.is_empty() => {
            commits.reverse();
            commits.into_iter().map(|commit| commit.sha).collect()
        }
        _ => vec![head_sha.to_string()],
    }
}
