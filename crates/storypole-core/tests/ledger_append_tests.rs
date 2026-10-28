//! Regression tests for atomic ledger appends (GB-147).
//!
//! The original defect: two concurrent writers spliced two JSON objects onto one line, and every
//! later command failed with "Corrupt ledger at line N: trailing characters".

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use serde_json::Value;
use storypole_core::ledger::{append_event, read_events};
use storypole_core::model::{LedgerEvent, TaskStatus};
use tempfile::tempdir;
use uuid::Uuid;

fn temp_ledger() -> (tempfile::TempDir, PathBuf) {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("ledger.jsonl");
    (dir, path)
}

fn status_event(n: usize) -> LedgerEvent {
    LedgerEvent::StatusChanged {
        task_id: Uuid::from_u128(n as u128),
        from: TaskStatus::Ready,
        to: TaskStatus::InProgress,
        reason: Some(format!(
            "event {n} with a deliberately longish payload to widen the write window"
        )),
        timestamp_ms: n as u64,
    }
}

#[test]
fn test_every_line_is_one_complete_json_object() {
    let (_dir, path) = temp_ledger();
    for n in 0..25 {
        append_event(&path, &status_event(n)).expect("append");
    }

    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.ends_with('\n'), "the file must end with a newline");
    for (i, line) in text.lines().enumerate() {
        serde_json::from_str::<Value>(line)
            .unwrap_or_else(|e| panic!("line {} is not valid JSON: {e}", i + 1));
    }
    assert_eq!(read_events(&path).expect("replay").len(), 25);
}

#[test]
fn test_concurrent_writers_never_splice_events() {
    let (_dir, path) = temp_ledger();
    let path = Arc::new(path);
    let mut handles = Vec::new();

    for writer in 0..8u128 {
        let path = Arc::clone(&path);
        handles.push(thread::spawn(move || {
            for n in 0..20usize {
                append_event(&path, &status_event(writer as usize * 100 + n)).expect("append");
            }
        }));
    }
    for handle in handles {
        handle.join().expect("writer joined");
    }

    let text = std::fs::read_to_string(&*path).expect("read");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 160, "every event must occupy exactly one line");
    for (i, line) in lines.iter().enumerate() {
        serde_json::from_str::<Value>(line)
            .unwrap_or_else(|e| panic!("line {} was spliced or truncated: {e}", i + 1));
    }
    assert_eq!(read_events(&path).expect("replay").len(), 160);
}

#[test]
fn test_append_refuses_a_partial_tail() {
    let (_dir, path) = temp_ledger();
    append_event(&path, &status_event(1)).expect("first append");

    // Simulate the corruption the race used to produce: a truncated object with no newline.
    let mut text = std::fs::read_to_string(&path).expect("read");
    text.truncate(text.len() - 5);
    std::fs::write(&path, &text).expect("corrupt");

    let err = append_event(&path, &status_event(2)).expect_err("must refuse to append");
    let message = err.to_string();
    assert!(
        message.contains("Corrupt ledger"),
        "expected a corrupt-ledger error, got: {message}"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("read"),
        text,
        "a refused append must not touch the file"
    );
}

#[test]
fn test_append_to_missing_or_empty_file_is_fine() {
    let (_dir, path) = temp_ledger();
    append_event(&path, &status_event(1)).expect("creates the file");
    assert_eq!(read_events(&path).expect("replay").len(), 1);
}
