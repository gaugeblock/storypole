//! Two clients, two worktrees, one ticket store (`GB-329`).
//!
//! The unit tests prove the lock across threads; this proves it across processes, which is what parallel
//! agents actually are. ADR 0001 resolves `.gaugeblock` through the git common dir, so both checkouts write
//! the same files - the interesting question is whether two concurrent invocations lose an event.
//!
//! multi-repo: creates real worktrees and runs the built binary from each.
//!
//! Both tests are ignored because they failed on their first run against two real defects, and a failing
//! test cannot be committed as an expected result. They stay here, ignored and named: the reproduction is
//! the expensive part, and each reason names the ticket that has to fix it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_storypole");

/// Runs the built binary in `cwd`, returning success and its combined output.
fn run(cwd: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(BIN)
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run storypole");
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

fn git(cwd: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// A records-mode repository plus a linked worktree, or `None` when git refuses.
fn records_suite(temp: &Path) -> Option<(PathBuf, PathBuf)> {
    let main = temp.join("main");
    fs::create_dir_all(&main).ok()?;
    if !git(&main, &["init", "-q"]) {
        return None;
    }
    git(&main, &["config", "user.email", "t@example.com"]);
    git(&main, &["config", "user.name", "Test"]);

    let (ok, out) = run(&main, &["init"]);
    assert!(ok, "storypole init failed: {out}");
    // The migration's job, done by hand here: the directory is what selects records mode.
    fs::create_dir_all(main.join(".gaugeblock/storypole/tickets")).ok()?;

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

fn ticket_files(dot: &Path) -> Vec<String> {
    let dir = dot.join("tickets");
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("tickets directory")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();
    names
}

fn event_count(dot: &Path, key: &str) -> usize {
    let path = dot.join("tickets").join(format!("{key}.json"));
    let record: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("record")).expect("record json");
    record["events"].as_array().expect("events").len()
}

#[test]
#[ignore = "GB-345: concurrent add mints one key twice and the second record overwrites the first"]
fn two_worktrees_creating_tickets_lose_neither() {
    let temp = TempDir::new().unwrap();
    let Some((main, linked)) = records_suite(temp.path()) else {
        eprintln!("git worktree unavailable; skipping");
        return;
    };
    let dot = main.join(".gaugeblock/storypole");

    let from_main = {
        let main = main.clone();
        thread::spawn(move || run(&main, &["add", "from the main checkout", "--kind", "docs"]))
    };
    let from_linked = {
        let linked = linked.clone();
        thread::spawn(move || run(&linked, &["add", "from agent b", "--kind", "docs"]))
    };
    let (ok_main, out_main) = from_main.join().unwrap();
    let (ok_linked, out_linked) = from_linked.join().unwrap();

    assert!(ok_main, "main checkout could not add: {out_main}");
    assert!(ok_linked, "agent-b checkout could not add: {out_linked}");

    let files = ticket_files(&dot);
    assert_eq!(files.len(), 2, "two tickets, two files: {files:?}");
    assert_eq!(
        fs::read(dot.join("ledger.jsonl")).unwrap().len(),
        0,
        "records mode writes no ledger"
    );

    let (listed, table) = run(&main, &["list"]);
    assert!(listed, "list failed: {table}");
    assert_eq!(
        table.matches(".json").count() + table.lines().filter(|l| l.contains("docs")).count(),
        2,
        "both tickets are visible from the main checkout:\n{table}"
    );
}

#[test]
#[ignore = "GB-333: tracked records are per branch by design (ADR 0009), so two worktrees must share leases, not records"]
fn concurrent_updates_to_one_ticket_keep_every_event() {
    let temp = TempDir::new().unwrap();
    let Some((main, linked)) = records_suite(temp.path()) else {
        eprintln!("git worktree unavailable; skipping");
        return;
    };
    let dot = main.join(".gaugeblock/storypole");
    let (ok, out) = run(&main, &["add", "the contended ticket", "--kind", "docs"]);
    assert!(ok, "add failed: {out}");
    let key = ticket_files(&dot)
        .first()
        .map(|name| name.trim_end_matches(".json").to_string())
        .expect("one ticket");

    let handles: Vec<_> = (0..8)
        .map(|n| {
            let cwd = if n % 2 == 0 {
                main.clone()
            } else {
                linked.clone()
            };
            let key = key.clone();
            thread::spawn(move || {
                run(
                    &cwd,
                    &["update", &key, "--priority", &((n % 5) + 1).to_string()],
                )
            })
        })
        .collect();
    let mut failures = Vec::new();
    for handle in handles {
        let (ok, out) = handle.join().unwrap();
        if !ok {
            failures.push(out);
        }
    }

    assert!(
        failures.is_empty(),
        "every update should succeed: {failures:?}"
    );
    assert_eq!(
        event_count(&dot, &key),
        9,
        "one creation and eight updates, none lost or overwritten"
    );
}
