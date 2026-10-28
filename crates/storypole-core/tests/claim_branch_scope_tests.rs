//! Regression tests for claim branch scoping (GB-137).
//!
//! A harness-level workspace store owns no git repository, so claiming a task must never move the
//! harness checkout; the branch belongs to the member named by `scope.repo`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use storypole_core::config::StorypoleConfig;
use storypole_core::model::task::TaskScope;
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::ops::workflow::{claim_task, resolve_task_repo};
use storypole_core::store::StorypoleStore;
use tempfile::tempdir;

/// Initialises a git repository on `main` with one commit.
fn init_repo(dir: &Path) {
    fs::create_dir_all(dir).expect("create repo dir");
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git run");
        assert!(out.status.success(), "git {args:?} failed: {out:?}");
    };
    run(&["init", "-q", "-b", "main"]);
    run(&[
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@example.com",
        "commit",
        "--allow-empty",
        "-q",
        "-m",
        "init",
    ]);
}

fn current_branch(dir: &Path) -> String {
    let out = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(dir)
        .output()
        .expect("git branch");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn test_member_store_owns_its_tasks() {
    let root = tempdir().expect("tempdir");
    fs::create_dir_all(root.path().join(".storypole")).expect("store dir");
    let mut store = StorypoleStore::init(root.path(), StorypoleConfig::default()).expect("init");
    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Member task".into(),
            ..Default::default()
        },
    )
    .expect("add task");

    let repo = resolve_task_repo(store.root_dir(), &task).expect("member store owns the task");
    assert_eq!(repo, store.root_dir());
}

#[test]
fn test_workspace_store_resolves_the_scoped_member_only() {
    let root = tempdir().expect("tempdir");
    fs::create_dir_all(root.path().join(".gaugeblock").join("storypole")).expect("store dir");
    init_repo(&root.path().join("alpha"));
    init_repo(&root.path().join("beta"));

    let mut store = StorypoleStore::init(root.path(), StorypoleConfig::default()).expect("init");
    // The helper must see the harness layout, not the member layout it just created.
    fs::remove_dir_all(root.path().join(".storypole")).ok();

    let scoped = add_task(
        &mut store,
        CreateTaskParams {
            title: "Alpha task".into(),
            scope: TaskScope {
                repo: Some("alpha".into()),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("add scoped task");

    let repo = resolve_task_repo(store.root_dir(), &scoped).expect("alpha owns the task");
    assert_eq!(repo, root.path().join("alpha"));

    let unscoped = add_task(
        &mut store,
        CreateTaskParams {
            title: "Harness task".into(),
            ..Default::default()
        },
    )
    .expect("add unscoped task");
    assert!(
        resolve_task_repo(store.root_dir(), &unscoped).is_none(),
        "a harness-level task belongs to no repository"
    );
}

#[test]
fn test_claim_creates_the_branch_in_the_member_repo() {
    let root = tempdir().expect("tempdir");
    init_repo(root.path());
    init_repo(&root.path().join("alpha"));
    fs::create_dir_all(root.path().join(".gaugeblock").join("storypole")).expect("store dir");

    let mut store = StorypoleStore::init(root.path(), StorypoleConfig::default()).expect("init");
    fs::remove_dir_all(root.path().join(".storypole")).ok();

    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Scoped claim".into(),
            scope: TaskScope {
                repo: Some("alpha".into()),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("add task");

    let briefing =
        claim_task(&mut store, task.id, "tester", Some("feat/alpha-work")).expect("claim task");

    assert_eq!(
        current_branch(&root.path().join("alpha")),
        "feat/alpha-work",
        "the member repository carries the lease branch"
    );
    assert_eq!(
        current_branch(root.path()),
        "main",
        "the harness checkout must never be moved by a claim"
    );
    let claim = briefing.task.active_claim.expect("claim recorded");
    assert_eq!(claim.git_branch.as_deref(), Some("feat/alpha-work"));
}
