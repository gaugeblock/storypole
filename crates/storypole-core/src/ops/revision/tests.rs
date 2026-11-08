#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use super::is_reachable_from_main;

/// Runs git in `dir`, asserting success so a broken fixture cannot look like a product defect.
fn git(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git spawns");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repo_with_main_commit(dir: &Path) -> String {
    std::fs::create_dir_all(dir).expect("dir");
    git(dir, &["init", "-q", "-b", "main"]);
    git(
        dir,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "--allow-empty",
            "-q",
            "-m",
            "init",
        ],
    );
    head(dir)
}

fn head(dir: &Path) -> String {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .expect("git rev-parse");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn a_claim_made_after_the_work_does_not_collapse_the_window() {
    // The regression: the claim's base is HEAD when a task is claimed after its work is committed, so
    // `base..head` was empty and the receipt fell back to a single commit. Eleven commits and one commit
    // both read `[1 commit(s)]`.
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path();
    let base = repo_with_main_commit(dir);
    git(dir, &["checkout", "-q", "-b", "feat/window"]);
    let first = commit_empty(dir, "first");
    let head = commit_empty(dir, "second");

    // claim_base == head, which is what a resumed ticket looks like.
    let (resolved, shas) = super::branch_delta(dir, Some(&head), Some(&head));

    assert_eq!(
        resolved.as_deref(),
        Some(base.as_str()),
        "the branch point is the base, not the claim that arrived late"
    );
    assert_eq!(shas.len(), 2, "both commits are in the window: {shas:?}");
    assert!(shas.contains(&first) && shas.contains(&head));
}

#[test]
fn a_claim_base_older_than_the_branch_point_widens_the_window() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path();
    let base = repo_with_main_commit(dir);
    git(dir, &["checkout", "-q", "-b", "feat/window"]);
    commit_empty(dir, "first");
    let head = commit_empty(dir, "second");

    let (resolved, shas) = super::branch_delta(dir, Some(&base), Some(&head));

    assert_eq!(resolved.as_deref(), Some(base.as_str()));
    assert_eq!(shas.len(), 2);
}

#[test]
fn a_window_with_no_namable_base_says_so() {
    // A repository with no `main` and no claim: one commit is all that can be said, and the receipt gets
    // a note so the count cannot be mistaken for a small change.
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path();
    std::fs::create_dir_all(dir).expect("dir");
    git(dir, &["init", "-q", "-b", "trunk"]);
    let head = commit_empty(dir, "only");

    let (resolved, shas) = super::branch_delta(dir, None, Some(&head));

    assert_eq!(resolved, None);
    assert_eq!(shas, vec![head]);
    assert!(super::unbounded_window_note().contains("HEAD only"));
}

/// Commits an empty commit and returns its sha.
fn commit_empty(dir: &Path, message: &str) -> String {
    git(
        dir,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "--allow-empty",
            "-q",
            "-m",
            message,
        ],
    );
    head(dir)
}

#[test]
fn a_commit_on_an_unmerged_branch_is_not_reachable_from_main() {
    // The GB-250 and GB-302 shape: verified with a receipt, and `main` never received the commit.
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path();
    let base = repo_with_main_commit(dir);
    assert_eq!(is_reachable_from_main(dir, &base), Some(true));

    git(dir, &["checkout", "-q", "-b", "feat/stranded"]);
    git(
        dir,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "--allow-empty",
            "-q",
            "-m",
            "work",
        ],
    );
    let stranded = head(dir);

    assert_eq!(
        is_reachable_from_main(dir, &stranded),
        Some(false),
        "a commit only on the feature branch is not reachable from main"
    );

    git(dir, &["checkout", "-q", "main"]);
    git(
        dir,
        &["merge", "-q", "--no-ff", "-m", "merge", "feat/stranded"],
    );
    assert_eq!(
        is_reachable_from_main(dir, &stranded),
        Some(true),
        "and it becomes reachable once merged"
    );
}

#[test]
fn a_directory_that_is_not_a_repository_answers_unknown_rather_than_merged() {
    let tmp = tempfile::tempdir().expect("tempdir");
    assert_eq!(
        is_reachable_from_main(tmp.path(), "0000000"),
        None,
        "a check that cannot run must not report success"
    );
}
