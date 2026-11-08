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
use storypole_core::StorypoleConfig;
use storypole_core::store::StorypoleStore;
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

/// A suite that declares `members` and holds a storypole ledger at its root.
fn suite_with_members(root: &Path, members: &[&str]) -> StorypoleStore {
    // The store must live in `.gaugeblock/storypole` for the suite layout to exist at all: `init` picks
    // `.storypole` when that directory is absent, which is the standalone shape.
    fs::create_dir_all(root.join(".gaugeblock").join("storypole")).expect("suite store dir");
    let store = StorypoleStore::init(root, StorypoleConfig::default()).expect("init suite store");
    fs::remove_dir_all(root.join(".storypole")).ok();
    let list: Vec<String> = members.iter().map(|m| format!("\"{m}\"")).collect();
    fs::write(
        root.join(".gaugeblock").join("config.toml"),
        format!("[suite]\nmembers = [{}]\n", list.join(", ")),
    )
    .expect("suite declaration");
    store
}

#[test]
fn a_member_repository_reads_the_suite_ledger_rather_than_its_own_stub() {
    // The defect: a member carries `.gaugeblock/storypole` holding an index and no ledger, so the upward
    // walk found it first and opened an empty store. `storypole verify` then reported "task not found"
    // from inside the repository where the work lives (`GB-316`).
    let tmp = tempdir().expect("tempdir");
    let suite_store = suite_with_members(tmp.path(), &["miter"]);

    let member = tmp.path().join("miter");
    fs::create_dir_all(member.join(".git")).expect("member checkout");
    fs::create_dir_all(member.join(".gaugeblock").join("storypole")).expect("stub dir");
    fs::write(
        member
            .join(".gaugeblock")
            .join("storypole")
            .join("index.sqlite"),
        b"",
    )
    .expect("stub index");
    let deep = member.join("crates").join("engine");
    fs::create_dir_all(&deep).expect("deep dir");

    let opened = StorypoleStore::open(&deep).expect("a member can open the suite's store");

    assert_eq!(
        opened.ledger_path(),
        suite_store.ledger_path(),
        "a stub is not a store, so the member reads the suite ledger"
    );
    assert_eq!(
        opened.root_dir(),
        tmp.path(),
        "and it knows which suite it is in"
    );
}

#[test]
fn a_member_with_a_ledger_of_its_own_keeps_it() {
    // The other side: a member repository used on its own has its own backlog, and the suite above it
    // must not take that over.
    let tmp = tempdir().expect("tempdir");
    let _suite = suite_with_members(tmp.path(), &["miter"]);

    let member = tmp.path().join("miter");
    fs::create_dir_all(member.join(".git")).expect("member checkout");
    let own = repo_with_store(&member);
    fs::write(own.ledger_path(), "").expect("own ledger");

    let opened = StorypoleStore::open(&member).expect("opens");

    assert_eq!(
        opened.ledger_path(),
        own.ledger_path(),
        "a member holding a ledger keeps it: the suite does not overwrite intent"
    );
}

#[test]
fn a_suite_that_declares_no_members_answers_for_none() {
    // The safe direction: a repository that cannot prove it belongs somewhere keeps its own ledger.
    let tmp = tempdir().expect("tempdir");
    let _suite_store = repo_with_store(tmp.path());
    let member = tmp.path().join("miter");
    fs::create_dir_all(member.join(".git")).expect("member checkout");
    let own = repo_with_store(&member);

    let opened = StorypoleStore::open(&member).expect("opens");

    assert_eq!(
        opened.ledger_path(),
        own.ledger_path(),
        "no declaration means no adoption"
    );
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
    // Refusing is the correct outcome too: nothing is recorded anywhere, which is visible.
    if let Ok(store) = opened {
        assert_ne!(
            store.ledger_path(),
            parent_store.ledger_path(),
            "a nested repository must not adopt its parent's ledger"
        );
    }
}
