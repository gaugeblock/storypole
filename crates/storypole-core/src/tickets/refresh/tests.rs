//! A pass over a store repairs the projections its writes left behind (`GB-349`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use gaugeblock_contracts::storypole::{TaskKind, TaskScope, TaskStatus};
use tempfile::tempdir;
use uuid::Uuid;

use super::*;
use crate::model::LedgerEvent;

/// A task with the fields a record actually reads.
fn task(id: Uuid, key: &str) -> crate::model::TaskNode {
    crate::model::TaskNode {
        id,
        ticket_key: key.to_string(),
        slug: format!("{key}-slug"),
        title: "a ticket".to_string(),
        description: "as written".to_string(),
        kind: TaskKind::Feature,
        status: TaskStatus::Ready,
        priority: 3,
        scope: TaskScope {
            repo: Some("storypole".to_string()),
            crate_name: None,
            module_path: None,
            target_files: Vec::new(),
            target_symbols: Vec::new(),
            labels: Vec::new(),
        },
        edges: Vec::new(),
        decisions: Vec::new(),
        acceptance_criteria: Vec::new(),
        plan: None,
        active_claim: None,
        latest_handoff: None,
        verification_receipt: None,
        created_at_epoch_ms: 1_000,
        updated_at_epoch_ms: 1_000,
        archived_at_epoch_ms: None,
    }
}

/// Writes a record the way the store did before the fix: projection left at creation, events current.
///
/// `serde_json` directly rather than `tickets::write`, which would repair it on the way in and leave the
/// test proving nothing.
fn write_stale(dot_dir: &Path, key: &str, archived: bool) {
    let id = Uuid::new_v4();
    let created = task(id, key);
    let mut events = vec![LedgerEvent::TaskCreated {
        task: created.clone(),
    }];
    if archived {
        events.push(LedgerEvent::TaskArchived {
            task_id: id,
            status: TaskStatus::Verified,
            timestamp_ms: 2_000,
        });
    }
    let record = crate::tickets::TicketRecord {
        ticket: created,
        events,
        external: None,
    };
    let dir = tickets::dir(dot_dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join(format!("{key}.json")),
        serde_json::to_vec_pretty(&record).unwrap(),
    )
    .unwrap();
}

#[test]
fn an_archived_ticket_whose_block_says_otherwise_is_reported_then_repaired() {
    let root = tempdir().unwrap();
    let dot = root.path().join(".gaugeblock").join("storypole");
    write_stale(&dot, "GB-1", true);
    write_stale(&dot, "GB-2", false);

    let dry = refresh_store(&dot, false).unwrap();
    assert_eq!(dry.scanned, 2);
    assert_eq!(
        dry.stale,
        vec!["GB-1".to_string()],
        "only the archived one disagrees"
    );
    assert!(dry.written.is_empty(), "a dry run writes nothing");

    let applied = refresh_store(&dot, true).unwrap();
    assert_eq!(applied.written, vec!["GB-1".to_string()]);

    // The file now agrees with itself, which is the whole point: a reader that never replays gets the
    // answer a reader that replays gets.
    let repaired = tickets::read(&tickets::path(&dot, "GB-1").unwrap()).unwrap();
    assert_eq!(repaired.ticket.status, TaskStatus::Verified);
    assert_eq!(repaired.ticket.archived_at_epoch_ms, Some(2_000));
    assert_eq!(repaired.ticket, repaired.replay().unwrap());

    // And a second pass finds nothing, so the repair is safe to leave in a loop or a hook.
    let again = refresh_store(&dot, true).unwrap();
    assert!(again.is_clean(), "second pass: {again:?}");
}

#[test]
fn a_record_that_cannot_be_read_is_reported_rather_than_skipped() {
    let root = tempdir().unwrap();
    let dot = root.path().join(".gaugeblock").join("storypole");
    let dir = tickets::dir(&dot);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("GB-9.json"), b"{ not a record").unwrap();

    let report = refresh_store(&dot, false).unwrap();
    assert_eq!(report.unreadable.len(), 1);
    assert_eq!(report.unreadable[0].0, "GB-9");
    assert!(
        !report.is_clean(),
        "an unreadable record is not a clean pass"
    );
}
