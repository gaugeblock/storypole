//! A receipt carries the change it verified, and says so when that change has been rewritten (`SP-41`).
//!
//! The defect these pin: a receipt recorded a window of SHAs and nothing else, so a rebase, a squash or a
//! force-push turned the record into a dangling pointer - the suite already had 44 edges whose target no
//! longer resolved. Content survives the rewrite; position does not.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use storypole_core::StorypoleConfig;
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::ops::revision::{capture_change, orphaned_window_warning, window_exists};
use storypole_core::ops::verification::verify_task;
use storypole_core::store::StorypoleStore;
use tempfile::tempdir;

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git spawns");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A repository with one commit on `main`, and a store inside it.
fn repo_with_store(root: &Path) -> StorypoleStore {
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.email", "t@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    std::fs::write(root.join("work.txt"), "one\n").expect("write");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "chore: initial"]);

    std::fs::create_dir_all(root.join(".gaugeblock").join("storypole").join("tickets"))
        .expect("store dir");
    let store = StorypoleStore::init(root, StorypoleConfig::default()).expect("init");
    std::fs::remove_dir_all(root.join(".storypole")).ok();
    store
}

#[test]
fn a_receipt_carries_the_diff_it_verified() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();
    let mut store = repo_with_store(root);
    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Change the file".into(),
            ..Default::default()
        },
    )
    .expect("task");

    // Work on a branch, so the window is a range rather than a single commit.
    git(root, &["checkout", "-q", "-b", "work"]);
    std::fs::write(root.join("work.txt"), "two\n").expect("write");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "feat: change the file"]);

    let receipt = verify_task(&mut store, task.id, None, false, false).expect("verify");

    let hash = receipt.diff_hash.expect("the change is hashed");
    assert_eq!(hash.len(), 40, "a git blob id, not an ad-hoc digest");
    assert!(
        hash.chars().all(|c| c.is_ascii_hexdigit()),
        "hex, got {hash}"
    );
    let diff = receipt.diff.expect("the diff is captured");
    assert!(diff.contains("-one"), "the removed line is present");
    assert!(diff.contains("+two"), "the added line is present");
    assert_eq!(
        receipt.window_intact,
        Some(true),
        "both ends of the window exist at write time"
    );
    assert!(
        receipt.base_commit_sha.is_some(),
        "and the window is still recorded beside the content"
    );
}

#[test]
fn a_window_that_no_longer_exists_is_detected_and_reported() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();
    let mut store = repo_with_store(root);
    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Certify something".into(),
            ..Default::default()
        },
    )
    .expect("task");
    let head = git(root, &["rev-parse", "HEAD"]);
    verify_task(&mut store, task.id, None, false, false).expect("verify");

    // A window naming a commit this repository does not have is exactly the post-force-push shape.
    let missing = "0123456789abcdef0123456789abcdef01234567";
    assert_eq!(
        window_exists(root, None, Some(missing)),
        Some(false),
        "an absent commit is reported absent, not assumed present"
    );
    assert_eq!(
        window_exists(root, None, Some(&head)),
        Some(true),
        "and a present one is reported present"
    );

    // The reader-side check names the receipt's content, because that is what now carries the evidence.
    let mut broken =
        storypole_core::index::query::get_task_by_id(store.conn(), task.id).expect("get");
    {
        let receipt = broken
            .verification_receipt
            .as_mut()
            .expect("the receipt was recorded");
        receipt.head_commit_sha = Some(missing.to_string());
    }
    let recorded_hash = broken
        .verification_receipt
        .as_ref()
        .and_then(|receipt| receipt.diff_hash.clone())
        .unwrap_or_default();
    let warning = orphaned_window_warning(root, &broken).expect("the orphaning is reported");
    assert!(
        warning.contains(&recorded_hash),
        "the warning names the diff hash, which is the surviving record: {warning}"
    );
}

#[test]
fn a_capture_with_no_commits_is_empty_rather_than_invented() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();
    let _store = repo_with_store(root);

    let captured = capture_change(root, None, None);
    assert!(captured.hash.is_none() && captured.diff.is_none() && captured.window_intact.is_none());
}
