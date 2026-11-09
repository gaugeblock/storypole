//! Records mode: one file per ticket is the only writer (`GB-329`).
//!
//! The property under test is that switching the layout changes *where* events go and nothing else: a
//! records-mode store must not touch `ledger.jsonl`, because writing both layouts is the divergence `GB-79`
//! documents.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use gaugeblock_contracts::storypole::{TaskKind, TaskScope, TaskStatus};
use gaugeblock_contracts::{LedgerEvent, TaskNode};
use storypole_core::config::StorypoleConfig;
use storypole_core::storage_mode::StorageMode;
use storypole_core::store::StorypoleStore;
use tempfile::TempDir;
use uuid::Uuid;

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

/// A store whose layout is records: initialise, then create the directory the migration would create.
fn records_store(temp: &TempDir) -> StorypoleStore {
    // No pre-creation of `.gaugeblock` any more: init writes there itself since GB-344.
    StorypoleStore::init(temp.path(), StorypoleConfig::default()).expect("init");
    fs::create_dir_all(
        temp.path()
            .join(".gaugeblock")
            .join("storypole")
            .join("tickets"),
    )
    .expect("tickets directory");
    let store = StorypoleStore::open_exact(temp.path()).expect("open");
    assert_eq!(store.storage_mode(), StorageMode::Records);
    store
}

#[test]
fn an_event_lands_in_a_record_and_the_ledger_stays_empty() {
    let temp = TempDir::new().unwrap();
    let mut store = records_store(&temp);
    let id = Uuid::from_u128(11);

    store
        .record_event(&LedgerEvent::TaskCreated {
            task: task(id, "GB-11"),
        })
        .expect("record the creation");

    let record = temp.path().join(".gaugeblock/storypole/tickets/GB-11.json");
    assert!(record.is_file(), "the ticket became a file");
    assert_eq!(
        fs::metadata(store.ledger_path()).unwrap().len(),
        0,
        "the ledger is not written in records mode"
    );

    let indexed: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(indexed, 1, "and the index still answers");
}

#[test]
fn every_event_for_one_ticket_lands_in_that_tickets_file() {
    let temp = TempDir::new().unwrap();
    let mut store = records_store(&temp);
    let id = Uuid::from_u128(12);
    store
        .record_event(&LedgerEvent::TaskCreated {
            task: task(id, "GB-12"),
        })
        .unwrap();

    // Resolving the owner of a non-creation event needs the index, which is why this is the interesting
    // case: the event names only a task id.
    store
        .record_event(&LedgerEvent::StatusChanged {
            task_id: id,
            from: TaskStatus::Ready,
            to: TaskStatus::InProgress,
            reason: None,
            timestamp_ms: 200,
        })
        .unwrap();

    let path = temp.path().join(".gaugeblock/storypole/tickets/GB-12.json");
    let record: storypole_core::tickets::TicketRecord =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(record.events.len(), 2, "creation and the move");
    assert_eq!(record.replay().unwrap().status, TaskStatus::InProgress);
    assert_eq!(
        fs::read_dir(temp.path().join(".gaugeblock/storypole/tickets"))
            .unwrap()
            .count(),
        1,
        "one ticket means one file"
    );
}

#[test]
fn two_files_holding_one_task_is_reported_rather_than_merged() {
    // The invariant that GB-79's divergence earned: the same task id in two records has no correct merge,
    // so it is corruption to name both paths and stop.
    let temp = TempDir::new().unwrap();
    let store = records_store(&temp);
    let id = Uuid::from_u128(13);
    let dir = temp.path().join(".gaugeblock/storypole/tickets");

    for key in ["GB-13", "GB-14"] {
        let record = storypole_core::tickets::TicketRecord::new(
            task(id, key),
            vec![LedgerEvent::TaskCreated {
                task: task(id, key),
            }],
        );
        storypole_core::tickets::write(&dir.join(format!("{key}.json")), &record).unwrap();
    }

    let error = storypole_core::tickets::read_all(store.dot_dir()).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("stored twice"), "{message}");
    assert!(
        message.contains("GB-13") && message.contains("GB-14"),
        "{message}"
    );
}

#[test]
fn a_record_named_for_a_different_ticket_is_reported() {
    let temp = TempDir::new().unwrap();
    let store = records_store(&temp);
    let dir = temp.path().join(".gaugeblock/storypole/tickets");
    let record = storypole_core::tickets::TicketRecord::new(
        task(Uuid::from_u128(15), "GB-15"),
        vec![LedgerEvent::TaskCreated {
            task: task(Uuid::from_u128(15), "GB-15"),
        }],
    );
    storypole_core::tickets::write(&dir.join("GB-999.json"), &record).unwrap();

    let error = storypole_core::tickets::read_all(store.dot_dir()).unwrap_err();
    assert!(error.to_string().contains("named GB-999"), "{error}");
}
