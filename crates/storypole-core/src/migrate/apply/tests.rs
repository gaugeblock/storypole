//! Applying a plan (`GB-331`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use gaugeblock_contracts::storypole::{TaskKind, TaskScope, TaskStatus};

use super::*;
use crate::ledger::read_events;
use crate::migrate::{self, Destination};

fn task(key: &str, repo: Option<&str>) -> TaskNode {
    let number: u128 = key.rsplit_once('-').unwrap().1.parse().unwrap();
    TaskNode {
        id: Uuid::from_u128(number),
        ticket_key: key.to_string(),
        slug: key.to_lowercase(),
        title: format!("{key} title"),
        description: "a description".to_string(),
        kind: TaskKind::Feature,
        status: TaskStatus::Ready,
        priority: 2,
        scope: TaskScope {
            repo: repo.map(str::to_string),
            ..TaskScope::default()
        },
        edges: Vec::new(),
        decisions: Vec::new(),
        acceptance_criteria: Vec::new(),
        plan: None,
        active_claim: None,
        latest_handoff: None,
        verification_receipt: None,
        created_at_epoch_ms: 10,
        updated_at_epoch_ms: 10,
        archived_at_epoch_ms: None,
    }
}

/// A ledger-mode store holding the given tasks, as a migration source.
fn ledger_store(root: &std::path::Path, tasks: Vec<TaskNode>) -> PathBuf {
    let dot = root.join(".gaugeblock").join("storypole");
    fs::create_dir_all(&dot).unwrap();
    let mut lines = String::new();
    for task in tasks {
        let event = LedgerEvent::TaskCreated { task };
        lines.push_str(&serde_json::to_string(&event).unwrap());
        lines.push('\n');
    }
    fs::write(dot.join("ledger.jsonl"), lines).unwrap();
    dot
}

#[test]
fn applying_a_plan_moves_tickets_and_archives_the_ledger() {
    let temp = tempfile::tempdir().unwrap();
    let source = ledger_store(
        temp.path(),
        vec![
            task("GB-1", Some("benchwork")),
            task("GB-2", Some("gaugeblock")),
            task("GB-3", None),
        ],
    );
    let member = temp
        .path()
        .join("benchwork")
        .join(".gaugeblock")
        .join("storypole");
    fs::create_dir_all(&member).unwrap();

    let state = crate::state::load(&source).unwrap();
    let plan = migrate::plan(&state, &["benchwork".to_string()], &|_| None);
    let member_dot = member.clone();
    let applied = super::apply(&source, &state, &plan, &|repo| {
        (repo == "benchwork").then(|| member_dot.clone())
    })
    .expect("apply");

    assert_eq!(applied.written, 3);
    assert_eq!(applied.moved, 1);
    assert!(
        applied.archive.is_file(),
        "the ledger is preserved at {}",
        applied.archive.display()
    );
    assert!(!source.join("ledger.jsonl").exists(), "and no longer live");
    assert_eq!(
        1,
        fs::read_dir(crate::tickets::dir(&member)).unwrap().count()
    );
    assert_eq!(
        2,
        fs::read_dir(crate::tickets::dir(&source)).unwrap().count(),
        "the suite-level tickets stay at the root"
    );
}

#[test]
fn an_unroutable_destination_stops_the_migration_before_the_ledger_is_touched() {
    // Phase order is the safety property: a plan that cannot be written must cost nothing, so the original
    // ledger has to still be there afterwards.
    let temp = tempfile::tempdir().unwrap();
    let source = ledger_store(temp.path(), vec![task("GB-1", Some("benchwork"))]);
    let before = read_events(&source.join("ledger.jsonl")).unwrap().len();

    let state = crate::state::load(&source).unwrap();
    let plan = migrate::plan(&state, &["benchwork".to_string()], &|_| None);
    let error =
        super::apply(&source, &state, &plan, &|_| None).expect_err("no store for the member");

    assert!(error.to_string().contains("has no store"), "{error}");
    assert_eq!(
        read_events(&source.join("ledger.jsonl")).unwrap().len(),
        before,
        "the ledger still holds the history"
    );
    assert!(
        !source.join("ledger-archive").exists(),
        "nothing was archived"
    );
}

#[test]
fn the_differential_replay_compares_what_a_reader_would_see() {
    // The migration may change how history is stored; it may not change what it says.
    let temp = tempfile::tempdir().unwrap();
    let source = ledger_store(temp.path(), vec![task("GB-1", Some("gaugeblock"))]);
    let state = crate::state::load(&source).unwrap();
    let plan = migrate::plan(&state, &[], &|_| None);
    assert_eq!(plan.assignments[0].destination, Destination::Root);

    super::apply(&source, &state, &plan, &|_| None).expect("apply");

    let after = crate::state::load(&source).unwrap();
    let before_task = state.tasks.values().next().unwrap();
    let after_task = after.tasks.values().next().unwrap();
    assert_eq!(before_task.ticket_key, after_task.ticket_key);
    assert_eq!(before_task.status, after_task.status);
    assert_eq!(before_task.description, after_task.description);
}
