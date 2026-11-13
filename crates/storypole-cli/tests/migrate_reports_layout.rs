//! `migrate` must describe both of its effects (`GB-357`).
//!
//! The command converts a store from one `ledger.jsonl` to a file per ticket, and also splits tickets across
//! repositories. Only the second was ever visible: a single-repository store saw `moving 0` and a closing line
//! about nothing being written, while applying it wrote every record and archived the ledger. The repository
//! that most needed the change was the one told there was nothing to do.
//!
//! The assertion that matters is not the wording but the arithmetic: the number the dry run promises must be the
//! number applying writes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_storypole");

/// Runs the binary, asserting success, and returns stdout.
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

/// A store on the Ledger layout holding `tickets` tickets, none of which names a repository.
fn ledger_store(tickets: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    run(dir.path(), &["init"]);
    for index in 0..tickets {
        run(dir.path(), &["add", &format!("ticket {index}")]);
    }
    dir
}

/// The store directory, whichever layout name `init` chose.
fn dot_dir(root: &Path) -> PathBuf {
    for candidate in [".gaugeblock/storypole", ".storypole"] {
        let dir = root.join(candidate);
        if dir.is_dir() {
            return dir;
        }
    }
    panic!("no store under {}", root.display());
}

/// The `key: value` numbers out of the dry run's JSON.
fn json_field(json: &str, field: &str) -> String {
    let needle = format!("\"{field}\":");
    let start = json
        .find(&needle)
        .unwrap_or_else(|| panic!("no {field} in {json}"))
        + needle.len();
    json[start..]
        .trim_start_matches('"')
        .chars()
        .take_while(|c| c.is_ascii_digit() || c.is_ascii_alphabetic())
        .collect()
}

#[test]
fn the_dry_run_names_the_transition_and_the_record_count() {
    let dir = ledger_store(3);

    let json = run(dir.path(), &["migrate", "--dry-run", "--json"]);
    assert_eq!(json_field(&json, "layout"), "Ledger");
    assert_eq!(
        json_field(&json, "target"),
        "Records",
        "the dry run must name the layout it will leave, not only the one it is in: {json}"
    );
    assert_eq!(json_field(&json, "records"), "3");
    assert_eq!(
        json_field(&json, "moving"),
        "0",
        "this store has one repository, so nothing moves - which is the case that read as a no-op"
    );

    let table = run(dir.path(), &["migrate", "--dry-run"]);
    assert!(
        table.contains("Ledger -> Records"),
        "the table must show the transition: {table}"
    );
    assert!(
        table.contains("3 record(s)"),
        "and the number of records: {table}"
    );
}

#[test]
fn a_store_with_no_moves_still_says_what_applying_would_do() {
    let dir = ledger_store(2);
    let table = run(dir.path(), &["migrate", "--dry-run"]);
    assert!(
        !table.contains("nothing was written and the ledger is untouched"),
        "the old closing line read as a no-op for exactly the store that needed the migration: {table}"
    );
    assert!(
        table.contains("change the layout"),
        "a zero-move store must be told the format changes: {table}"
    );
}

#[test]
fn the_promised_record_count_is_what_applying_writes() {
    // The wording is the symptom; this is the defect. If the preview and the write disagree, a reader who trusts
    // the dry run is misinformed no matter how clearly it is phrased.
    let dir = ledger_store(3);
    let promised: usize = json_field(
        &run(dir.path(), &["migrate", "--dry-run", "--json"]),
        "records",
    )
    .parse()
    .expect("records is a number");

    run(dir.path(), &["migrate"]);
    let tickets = dot_dir(dir.path()).join("tickets");
    let written = fs::read_dir(&tickets)
        .expect("a tickets directory after migrating")
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .count();

    assert_eq!(promised, written, "the dry run promised {promised}");
    assert!(
        dot_dir(dir.path())
            .join("ledger-archive/ledger-migrated.jsonl")
            .is_file(),
        "and the ledger is archived, which is the other half the dry run now names"
    );
}

#[test]
fn help_documents_both_effects() {
    let dir = ledger_store(0);
    let help = run(dir.path(), &["migrate", "--help"]);
    assert!(
        help.contains("Records layout"),
        "the verb that performs the layout migration must say so: {help}"
    );
    assert!(
        help.contains("repositories that own them"),
        "and must keep saying it splits the tickets: {help}"
    );
}

#[test]
fn workspace_distribute_no_longer_reads_as_the_layout_migration() {
    let dir = ledger_store(0);
    let help = run(dir.path(), &["workspace", "--help"]);
    assert!(
        !help.contains("root ledger"),
        "the root-ledger wording predates the Records layout and makes distribute read like migrate: {help}"
    );
}
