//! `add` and `update` must read identical arguments identically (`GB-359`).
//!
//! The original defect was not a missing split but a disagreement: `update` split `--file a,b` on commas and
//! `add` stored it as one path, so a record created the way people create records carried a scope entry that
//! exists nowhere - `["AGENTS.md,.gaugeblock/config.toml"]` for a ticket naming two files. It survived until
//! somebody read the file, because a populated-looking scope is not obviously wrong.
//!
//! These tests run the built binary twice with the same arguments and compare what was stored, which is the
//! only comparison that would have caught it: unit-testing each verb's parser separately passes while the two
//! parsers disagree.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_storypole");

/// A store initialised in a temp directory.
fn store() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = Command::new(BIN)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("storypole spawns");
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir
}

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

/// A ticket as the CLI reads it back.
///
/// Read through `show` rather than from `tickets/<KEY>.json`: a fresh store may be in Index storage and write no
/// records at all, and the assertion is about what the verbs stored, not about which layout stored it.
fn task_of(root: &Path, key: &str) -> serde_json::Value {
    let out = run(root, &["show", key, "--json"]);
    let value: serde_json::Value = serde_json::from_str(&out).expect("show --json is JSON");
    value["briefing"]["task"].clone()
}

/// The `scope` object of a ticket.
fn scope_of(root: &Path, key: &str) -> serde_json::Value {
    task_of(root, key)["scope"].clone()
}

/// The key out of `Created task SP-1 [Bug] P3: ...`.
fn key_from_add(stdout: &str) -> String {
    stdout
        .split_whitespace()
        .nth(2)
        .unwrap_or_else(|| panic!("no key in {stdout}"))
        .to_string()
}

#[test]
fn identical_arguments_produce_identical_scope() {
    let dir = store();

    // Created with the comma form, which is what a person types.
    let added = run(
        dir.path(),
        &[
            "add",
            "one",
            "--files",
            "a.rs,b.rs",
            "--symbols",
            "alpha,beta",
            "--labels",
            "storypole,cli",
        ],
    );
    let created = key_from_add(&added);

    // Created bare, then updated with the same arguments.
    let plain = run(dir.path(), &["add", "two"]);
    let updated = key_from_add(&plain);
    run(
        dir.path(),
        &[
            "update",
            &updated,
            "--files",
            "a.rs,b.rs",
            "--symbols",
            "alpha,beta",
            "--labels",
            "storypole,cli",
        ],
    );

    let from_add = scope_of(dir.path(), &created);
    let from_update = scope_of(dir.path(), &updated);
    assert_eq!(
        from_add["target_files"], from_update["target_files"],
        "add and update must split --files the same way"
    );
    assert_eq!(
        from_add["target_symbols"], from_update["target_symbols"],
        "and --symbols"
    );
    assert_eq!(from_add["labels"], from_update["labels"], "and --labels");
    // The specific damage: one concatenated entry instead of two usable ones.
    assert_eq!(
        from_add["target_files"],
        serde_json::json!(["a.rs", "b.rs"])
    );
    assert_eq!(from_add["labels"], serde_json::json!(["storypole", "cli"]));
}

#[test]
fn the_singular_spelling_is_accepted_by_both_verbs() {
    // `update` documented `--file` and `add` rejected it, so a person switching verbs hit a parse error for an
    // argument they had just used successfully. Both spellings now reach the same field.
    let dir = store();
    let added = run(
        dir.path(),
        &["add", "one", "--file", "a.rs", "--file", "b.rs"],
    );
    let created = key_from_add(&added);
    assert_eq!(
        scope_of(dir.path(), &created)["target_files"],
        serde_json::json!(["a.rs", "b.rs"]),
        "repetition still works, and it is the same field as the plural"
    );

    let plain = run(dir.path(), &["add", "two"]);
    let updated = key_from_add(&plain);
    run(dir.path(), &["update", &updated, "--files", "a.rs,b.rs"]);
    assert_eq!(
        scope_of(dir.path(), &created)["target_files"],
        scope_of(dir.path(), &updated)["target_files"],
        "a repeated singular and a comma-separated plural agree"
    );
}

#[test]
fn criteria_stay_repeatable_only_because_they_are_prose() {
    // Deliberately NOT comma-split (`GB-250`): one criterion containing a comma is one criterion. This asserts
    // the asymmetry is a decision rather than an oversight left behind by the fix above.
    let dir = store();
    let added = run(
        dir.path(),
        &[
            "add",
            "one",
            "--criterion",
            "it works, and it says so",
            "--criterion",
            "second",
        ],
    );
    let created = key_from_add(&added);
    let task = task_of(dir.path(), &created);
    assert_eq!(task["scope"]["target_files"], serde_json::json!([]));
    assert_eq!(
        task["acceptance_criteria"],
        serde_json::json!(["it works, and it says so", "second"]),
        "the comma stayed inside the criterion"
    );
}
