//! Archiving keeps the status a task earned, so a status filter overcounts open work (`GB-360`).
//!
//! Measured twice in this suite. A sweep globbed `tickets/*.json`, filtered `status == "triage"`, and reported
//! eight open rows where the store held two - five of them archived before the sweep ran. A board count read
//! `status` from the same blocks and reported 148 active tickets where replay said 78 (`GB-349`). The projection
//! was right both times; the reader was not.
//!
//! These tests pin the row shape that makes the mistake avoidable: `list --json --include-archived` emits
//! `archived_at_epoch_ms` beside `status`, so the archive marker is what a filter must use.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_storypole");

/// Runs the binary and returns stdout, asserting success.
fn run(dir: &Path, args: &[&str]) -> String {
    let output = Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .output()
        .expect("storypole spawns");
    assert!(
        output.status.success(),
        "storypole {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// A store holding one ticket, archived before it was ever worked on, plus the status it had at that moment.
fn store_with_archived_ticket() -> (tempfile::TempDir, String, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    run(dir.path(), &["init"]);
    let added = run(dir.path(), &["add", "a ticket that was never worked on"]);
    let key = added
        .split_whitespace()
        .nth(2)
        .unwrap_or_else(|| panic!("no key in {added}"))
        .to_string();
    let before: serde_json::Value =
        serde_json::from_str(&run(dir.path(), &["show", &key, "--json"]))
            .expect("show --json is JSON");
    let status = before["briefing"]["task"]["status"]
        .as_str()
        .unwrap_or_else(|| panic!("no status in {before}"))
        .to_string();
    run(dir.path(), &["archive", &key]);
    (dir, key, status)
}

/// The `tasks` array of a `list --json` answer.
fn rows(json: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(json).expect("list --json is JSON")["tasks"].clone()
}

#[test]
fn an_archived_ticket_keeps_the_status_it_earned() {
    let (dir, key, before) = store_with_archived_ticket();
    let all = rows(&run(dir.path(), &["list", "--json", "--include-archived"]));

    let row = all
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["ticket_key"] == key.as_str())
        .unwrap_or_else(|| panic!("{key} is missing from an --include-archived listing"));

    assert_eq!(
        row["status"].as_str(),
        Some(before.as_str()),
        "archive does not rewrite the status: this is the fact that made a status filter overcount"
    );
    assert!(
        !row["archived_at_epoch_ms"].is_null(),
        "and the archive marker is present in the same row, which is what a filter must use instead"
    );
}

#[test]
fn the_default_listing_hides_it_so_the_marker_is_the_only_way_to_see_both_facts() {
    let (dir, key, _) = store_with_archived_ticket();

    let default = rows(&run(dir.path(), &["list", "--json"]));
    assert!(
        !default
            .as_array()
            .expect("rows")
            .iter()
            .any(|row| row["ticket_key"] == key.as_str()),
        "the default listing hides archived work, so a reader that globs files is the only one that sees it - \
         which is exactly how a sweep counted it as open"
    );

    // The supported query: one round trip, both facts, no file reading.
    let all = rows(&run(dir.path(), &["list", "--json", "--include-archived"]));
    let open = all
        .as_array()
        .expect("rows")
        .iter()
        .filter(|row| row["archived_at_epoch_ms"].is_null())
        .count();
    assert_eq!(open, 0, "the store holds no open work, and says so");
}
