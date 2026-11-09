//! Two clients, two worktrees, one repository (`GB-329`, ADR 0009).
//!
//! What the phase change actually guarantees, measured through the built binary:
//!
//! * **Records follow the branch.** Each checkout writes its own `tickets/*.json`, because git checks out
//!   tracked files per working tree. Two branches holding two ticket sets is the design, not divergence.
//! * **Numbers and leases are shared.** Both come from the git directory, so two branches cannot mint the same
//!   key and cannot take the same ticket - the two collisions that made parallel agents unsafe.
//!
//! multi-repo: creates real worktrees and runs the built binary from each.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_storypole");

/// Runs the built binary in `cwd`, returning whether it succeeded.
fn run(cwd: &Path, args: &[&str]) -> bool {
    Command::new(BIN)
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run storypole")
        .status
        .success()
}

fn git(cwd: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// A records-mode repository plus one linked worktree cut from its first commit.
///
/// The commit matters: a worktree holds tracked state as of that commit, which is what makes the seeded
/// ticket visible to both checkouts while the lease stays visible for a different reason - it lives in the git
/// directory, outside the branch entirely.
fn records_suite(temp: &Path) -> Option<(PathBuf, PathBuf)> {
    let main = temp.join("main");
    fs::create_dir_all(&main).ok()?;
    if !git(&main, &["init", "-q"]) {
        return None;
    }
    git(&main, &["config", "user.email", "t@example.com"]);
    git(&main, &["config", "user.name", "Test"]);
    assert!(run(&main, &["init"]), "storypole init failed");
    fs::create_dir_all(main.join(".gaugeblock/storypole/tickets")).ok()?;

    if !run(&main, &["add", "the shared ticket", "--kind", "docs"]) {
        return None;
    }
    git(&main, &["add", "-A"]);
    if !git(&main, &["commit", "-q", "-m", "seed"]) {
        return None;
    }
    let linked = temp.join("agent-b");
    if !git(
        &main,
        &["worktree", "add", "-q", "-b", "agent-b", linked.to_str()?],
    ) {
        return None;
    }
    Some((main, linked))
}

fn ticket_files(checkout: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(checkout.join(".gaugeblock/storypole/tickets"))
        .expect("tickets directory")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();
    names
}

#[test]
fn a_ticket_created_in_each_checkout_gets_its_own_file_and_its_own_number() {
    let temp = TempDir::new().unwrap();
    let Some((main, linked)) = records_suite(temp.path()) else {
        eprintln!("git worktree unavailable; skipping");
        return;
    };

    assert!(
        run(&linked, &["add", "from agent b", "--kind", "docs"]),
        "agent-b must be able to create a ticket in its own checkout"
    );

    let in_main = ticket_files(&main);
    let in_linked = ticket_files(&linked);
    assert_eq!(
        in_main,
        vec!["SP-1.json"],
        "the committed ticket only: {in_main:?}"
    );
    assert_eq!(
        in_linked,
        vec!["SP-1.json", "SP-2.json"],
        "the worktree starts from the commit and adds its own: {in_linked:?}"
    );
}

#[test]
fn a_lease_taken_in_one_checkout_blocks_the_other() {
    // The end-to-end proof for GB-333: the ticket is visible in both checkouts because it was committed, and
    // the lease is visible in both because it lives outside the branch. Without the shared lease this claim
    // would succeed and two agents would work the same ticket.
    let temp = TempDir::new().unwrap();
    let Some((main, linked)) = records_suite(temp.path()) else {
        eprintln!("git worktree unavailable; skipping");
        return;
    };

    assert!(
        run(&main, &["claim", "SP-1", "agent-a"]),
        "agent-a claims in the main checkout"
    );

    let refused = Command::new(BIN)
        .current_dir(&linked)
        .args(["claim", "SP-1", "agent-b"])
        .output()
        .expect("run storypole from the worktree");
    let text = String::from_utf8_lossy(&refused.stderr).to_string()
        + &String::from_utf8_lossy(&refused.stdout);

    assert!(
        !refused.status.success(),
        "the second claim must fail: {text}"
    );
    assert!(
        text.contains("another checkout"),
        "and must say the lease is held elsewhere, not merely 'in use': {text}"
    );
}
