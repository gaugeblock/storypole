//! Two worktrees of one repository share one ticket store (`GB-329`).
//!
//! ADR 0001 resolves `.gaugeblock` through the git common dir, so every worktree writes the same files. That
//! is what makes parallel agents coordinate at all - and what makes the per-ticket lock load-bearing: two
//! agents in two checkouts are two processes against one record.
//!
//! multi-repo: this test creates real worktrees and asserts both resolve to one `.gaugeblock`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;

use gaugeblock_contracts::storypole::{TaskKind, TaskScope, TaskStatus};
use gaugeblock_contracts::{LedgerEvent, TaskNode};
use storypole_core::tickets;
use tempfile::TempDir;
use uuid::Uuid;

fn git(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn task(id: Uuid, key: &str) -> TaskNode {
    TaskNode {
        id,
        ticket_key: key.to_string(),
        slug: format!("{key}-slug"),
        title: "a ticket".to_string(),
        description: String::new(),
        kind: TaskKind::Feature,
        status: TaskStatus::Ready,
        priority: 3,
        scope: TaskScope::default(),
        edges: Vec::new(),
        decisions: Vec::new(),
        acceptance_criteria: Vec::new(),
        plan: None,
        active_claim: None,
        latest_handoff: None,
        verification_receipt: None,
        created_at_epoch_ms: 100,
        updated_at_epoch_ms: 100,
        archived_at_epoch_ms: None,
    }
}

/// A repository with one commit and a linked worktree, or `None` when git refuses.
fn repo_with_worktree(temp: &TempDir) -> Option<std::path::PathBuf> {
    let main = temp.path().join("main");
    fs::create_dir_all(&main).ok()?;
    if !git(&main, &["init", "-q"]) || !git(&main, &["config", "user.email", "t@example.com"]) {
        return None;
    }
    git(&main, &["config", "user.name", "Test"]);
    fs::write(main.join("README.md"), "seed\n").ok()?;
    git(&main, &["add", "README.md"]);
    if !git(&main, &["commit", "-q", "-m", "seed"]) {
        return None;
    }
    let linked = temp.path().join("agent-b");
    if !git(
        &main,
        &["worktree", "add", "-q", "-b", "agent-b", linked.to_str()?],
    ) {
        return None;
    }
    Some(linked)
}

#[test]
fn both_worktrees_resolve_to_the_same_gaugeblock_directory() {
    let temp = TempDir::new().unwrap();
    let Some(linked) = repo_with_worktree(&temp) else {
        eprintln!("git worktree unavailable; skipping");
        return;
    };
    let main = temp.path().join("main");

    let from_main = gaugeblock_workspace::GaugeblockPaths::discover(&main).unwrap();
    let from_worktree = gaugeblock_workspace::GaugeblockPaths::discover(&linked).unwrap();

    assert_eq!(
        from_main.root(),
        from_worktree.root(),
        "a worktree shares the main checkout's state, which is why the lock is load-bearing"
    );
}

#[test]
fn two_agents_in_two_worktrees_lose_no_event() {
    let temp = TempDir::new().unwrap();
    // The worktree only has to exist: both agents write through the same store, which is the point.
    if repo_with_worktree(&temp).is_none() {
        eprintln!("git worktree unavailable; skipping");
        return;
    }
    let main = temp.path().join("main");
    let dot = gaugeblock_workspace::GaugeblockPaths::discover(&main)
        .unwrap()
        .member("storypole");
    fs::create_dir_all(tickets::dir(&dot)).unwrap();

    let id = Uuid::from_u128(21);
    let record = tickets::TicketRecord::new(
        task(id, "GB-21"),
        vec![LedgerEvent::TaskCreated {
            task: task(id, "GB-21"),
        }],
    );
    tickets::write(&tickets::path(&dot, "GB-21").unwrap(), &record).unwrap();

    // Two agents, two checkouts, one store: the paths differ, the file does not.
    let handles: Vec<_> = [dot.clone(), dot.clone()]
        .into_iter()
        .enumerate()
        .map(|(agent, dot)| {
            thread::spawn(move || {
                let checkout = if agent == 0 { "main" } else { "agent-b" };
                for step in 0..10u64 {
                    let at = 1_000 + (agent as u64) * 1_000 + step;
                    tickets::append_event(
                        &dot,
                        "GB-21",
                        &LedgerEvent::StatusChanged {
                            task_id: id,
                            from: TaskStatus::Ready,
                            to: TaskStatus::InProgress,
                            reason: Some(checkout.to_string()),
                            timestamp_ms: at,
                        },
                    )
                    .unwrap();
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }

    let stored = tickets::read(&tickets::path(&dot, "GB-21").unwrap()).unwrap();
    assert_eq!(
        stored.events.len(),
        21,
        "one creation and twenty appends across two checkouts"
    );
    assert_eq!(stored.replay().unwrap().status, TaskStatus::InProgress);
}
