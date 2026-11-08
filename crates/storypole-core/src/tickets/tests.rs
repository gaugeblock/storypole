//! Ticket records, ordering and locking (`GB-329`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::thread;

use tempfile::tempdir;
use uuid::Uuid;

use super::*;
use crate::model::LedgerEvent;
use gaugeblock_contracts::storypole::{TaskKind, TaskScope, TaskStatus};

/// A task with the fields a record actually reads.
fn task(id: Uuid, key: &str, created: u64) -> TaskNode {
    TaskNode {
        id,
        ticket_key: key.to_string(),
        slug: format!("{key}-slug"),
        title: "a ticket".to_string(),
        description: String::new(),
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
        created_at_epoch_ms: created,
        updated_at_epoch_ms: created,
        archived_at_epoch_ms: None,
    }
}

/// A creation event.
fn created(id: Uuid, key: &str, at: u64) -> LedgerEvent {
    LedgerEvent::TaskCreated {
        task: task(id, key, at),
    }
}

/// A status change at `at`.
fn moved(id: Uuid, to: TaskStatus, at: u64) -> LedgerEvent {
    LedgerEvent::StatusChanged {
        task_id: id,
        from: TaskStatus::Ready,
        to,
        reason: None,
        timestamp_ms: at,
    }
}

#[test]
fn a_record_round_trips_and_leaves_no_temporary_file() {
    let temp = tempdir().unwrap();
    let dot = temp.path();
    let id = Uuid::from_u128(1);
    let record = TicketRecord::new(task(id, "GB-1", 10), vec![created(id, "GB-1", 10)]);

    let path = super::path(dot, "GB-1").unwrap();
    write(&path, &record).unwrap();

    assert_eq!(
        read(&path).unwrap(),
        record,
        "what was written is what comes back"
    );
    let leftovers: Vec<_> = fs::read_dir(dot.join("tickets"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "rename consumed the temporary: {leftovers:?}"
    );
}

#[test]
fn concurrent_appends_lose_no_event() {
    // The property the lock exists for. A whole-file record is a read-modify-write, so without the lock
    // the last writer wins and the other's event vanishes - silently, which is why it needs a test.
    let temp = tempdir().unwrap();
    let dot = temp.path().to_path_buf();
    let id = Uuid::from_u128(2);
    let path = super::path(&dot, "GB-2").unwrap();
    write(
        &path,
        &TicketRecord::new(task(id, "GB-2", 10), vec![created(id, "GB-2", 10)]),
    )
    .unwrap();

    let mut handles = Vec::new();
    for writer in 0..8u64 {
        let dot = dot.clone();
        handles.push(thread::spawn(move || {
            for step in 0..5u64 {
                let at = 1_000 + writer * 100 + step;
                append_event(&dot, "GB-2", &moved(id, TaskStatus::InProgress, at)).unwrap();
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }

    let record = read(&path).unwrap();
    assert_eq!(record.events.len(), 41, "one creation plus forty appends");
    let stamps: Vec<u64> = record.events.iter().map(event_timestamp).collect();
    let mut unique = stamps.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        41,
        "every writer's event survived exactly once"
    );
}

#[test]
fn replay_ignores_the_order_events_were_stored_in() {
    // The merge case: two branches wrote the same ticket and their histories interleaved on merge. Order
    // has to come from the clock, or a stale status can win because its line landed last.
    let temp = tempdir().unwrap();
    let dot = temp.path();
    let id = Uuid::from_u128(3);
    let mut events = vec![
        moved(id, TaskStatus::Verified, 300),
        created(id, "GB-3", 100),
        moved(id, TaskStatus::InProgress, 200),
    ];

    order_events(&mut events);

    let stamps: Vec<u64> = events.iter().map(event_timestamp).collect();
    assert_eq!(
        stamps,
        vec![100, 200, 300],
        "creation first, then each move"
    );

    let record = TicketRecord::new(task(id, "GB-3", 100), events);
    let replayed = record.replay().unwrap();
    assert_eq!(replayed.status, TaskStatus::Verified, "the last event wins");
    let path = super::path(dot, "GB-3").unwrap();
    write(&path, &record).unwrap();
    assert_eq!(
        read(&path).unwrap().replay().unwrap().status,
        TaskStatus::Verified
    );
}

#[test]
fn a_key_that_could_escape_the_ticket_directory_is_refused() {
    let temp = tempdir().unwrap();
    for key in ["", "../escape", "a/b", "a\\b", ".."] {
        assert!(
            super::path(temp.path(), key).is_err(),
            "'{key}' must not resolve to a file"
        );
    }
}

#[test]
fn the_lock_is_released_once_the_append_finishes() {
    let temp = tempdir().unwrap();
    let dot = temp.path();
    let id = Uuid::from_u128(4);
    let path = super::path(dot, "GB-4").unwrap();
    write(
        &path,
        &TicketRecord::new(task(id, "GB-4", 10), vec![created(id, "GB-4", 10)]),
    )
    .unwrap();

    append_event(dot, "GB-4", &moved(id, TaskStatus::InProgress, 20)).unwrap();
    append_event(dot, "GB-4", &moved(id, TaskStatus::Verified, 30)).unwrap();

    assert!(
        !dot.join("tickets").join("GB-4.lock").exists(),
        "locks do not linger"
    );
    assert_eq!(read(&path).unwrap().events.len(), 3);
}
