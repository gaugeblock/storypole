//! Two repositories in scope, which is the only way to see cross-repository leakage (`GB-305`).
//!
//! `StorypoleStore::open` resolves by walking upwards to the nearest enclosing store. That is correct for a
//! repository and dangerous for a suite: a repository with no store of its own silently inherits an
//! ancestor's, so work in the child is recorded in the parent's ledger and nothing reports it. With one
//! repository per test that behaviour is invisible, because there is no ancestor to inherit from.

//! multi-repo: two repositories in scope, which is the only way to see cross-repository behaviour.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;
use storypole_core::store::StorypoleStore;
use storypole_core::StorypoleConfig;
use tempfile::tempdir;

/// Runs git in `dir`, asserting success so a broken fixture cannot masquerade as a product defect.
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

/// A repository with its own initialised store.
fn repo_with_store(root: &Path) -> StorypoleStore {
    fs::create_dir_all(root).expect("create repo dir");
    StorypoleStore::init(root, StorypoleConfig::default()).expect("init store")
}

#[test]
fn sibling_repositories_do_not_share_a_ledger() {
    let tmp = tempdir().expect("tempdir");
    let alpha = tmp.path().join("alpha");
    let beta = tmp.path().join("beta");
    let store_a = repo_with_store(&alpha);
    let store_b = repo_with_store(&beta);

    // Distinct ledgers on disk is the claim; a shared path would show up as one file.
    assert_ne!(
        store_a.ledger_path(),
        store_b.ledger_path(),
        "two repositories must not resolve to one ledger"
    );
}

#[test]
fn a_nested_repository_without_a_store_is_not_silently_recorded_in_its_parent() {
    let tmp = tempdir().expect("tempdir");
    let parent = tmp.path().join("parent");
    let child = parent.join("child");
    let parent_store = repo_with_store(&parent);
    fs::create_dir_all(&child).expect("create child dir");
    // A directory inside a repository is not a repository. Without this the child *is* the parent as far as
    // git is concerned, and inheriting the parent's ledger is correct behaviour rather than the defect.
    git(&child, &["init", "-b", "main"]);

    let opened = StorypoleStore::open(&child);

    // The defect this asserts against: an upward walk finds the parent's store and the child's work lands in
    // the parent's ledger, with the ledger looking exactly as it should.
    match opened {
        Ok(store) => assert_ne!(
            store.ledger_path(),
            parent_store.ledger_path(),
            "a nested repository must not adopt its parent's ledger"
        ),
        // Refusing is the correct outcome: nothing is recorded anywhere, which is visible.
        Err(_) => {}
    }
}
