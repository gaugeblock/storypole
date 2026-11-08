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
