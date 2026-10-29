//! Tests for the ledger integrity audit (GB-178).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use storypole_core::config::StorypoleConfig;
use storypole_core::ledger::audit::{audit_ledgers, discover_ledgers, format_ledger_audit};
use storypole_core::model::task::TaskScope;
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::store::StorypoleStore;

fn store_with_task() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(tmp.path().join(".gaugeblock").join("storypole")).expect("layout");
    let mut store = StorypoleStore::init(tmp.path(), StorypoleConfig::default()).expect("init");
    add_task(
        &mut store,
        CreateTaskParams {
            title: "Audit subject".into(),
            scope: TaskScope {
                repo: Some("gaugeblock".into()),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("task");

    let ledger = tmp
        .path()
        .join(".gaugeblock")
        .join("storypole")
        .join("ledger.jsonl");
    (tmp, ledger)
}

#[test]
fn a_healthy_ledger_replays_without_findings() {
    let (tmp, ledger) = store_with_task();
    let before = fs::read_to_string(&ledger).expect("read");

    let audit = audit_ledgers(tmp.path()).expect("audit");

    assert_eq!(audit.unparsable, 0);
    assert_eq!(audit.duplicates, 0);
    assert!(audit.is_structurally_sound());
    assert_eq!(
        audit.total_events,
        before.lines().filter(|l| !l.trim().is_empty()).count()
    );
    assert_eq!(audit.tasks, 1, "the created task replays back");
    assert_eq!(audit.status_counts.get("ready").copied(), Some(1));
}

#[test]
fn a_duplicate_line_and_a_bad_line_are_both_reported() {
    let (tmp, ledger) = store_with_task();
    let before = fs::read_to_string(&ledger).expect("read");
    let first = before.lines().next().expect("a first event");
    fs::write(&ledger, format!("{before}{first}\n{{not json}}\n")).expect("write");

    let audit = audit_ledgers(tmp.path()).expect("audit");

    assert_eq!(audit.duplicates, 1, "the repeated event is counted once");
    assert_eq!(audit.unparsable, 1, "the bad line is counted");
    assert!(!audit.is_structurally_sound());
    assert_eq!(
        audit.tasks, 1,
        "a corrupt line must not lose the task that still replays"
    );
}

#[test]
fn discovery_covers_the_root_and_member_ledgers() {
    let (tmp, _ledger) = store_with_task();
    let member = tmp.path().join("member").join(".storypole");
    fs::create_dir_all(&member).expect("member dir");
    fs::write(member.join("ledger.jsonl"), "").expect("member ledger");

    let found = discover_ledgers(tmp.path());
    assert_eq!(found.len(), 2, "root plus one member: {found:?}");
    assert!(found[0].ends_with("ledger.jsonl"));

    let rendered = format_ledger_audit(&audit_ledgers(tmp.path()).expect("audit"));
    assert!(rendered.contains("events across"), "got: {rendered}");
}
