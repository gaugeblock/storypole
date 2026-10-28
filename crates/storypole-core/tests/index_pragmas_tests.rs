//! Regression tests for the SQLite index performance PRAGMAs (GB-132).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use storypole_core::index::open_and_sync_index;
use tempfile::tempdir;

#[test]
fn test_index_connection_uses_wal_and_normal_sync() {
    let dir = tempdir().expect("tempdir");
    let ledger = dir.path().join("ledger.jsonl");
    fs::write(&ledger, "").expect("write empty ledger");

    let conn = open_and_sync_index(&dir.path().join("index.sqlite"), &ledger).expect("open index");

    let journal_mode: String = conn
        .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
        .expect("read journal_mode");
    assert_eq!(
        journal_mode.to_lowercase(),
        "wal",
        "index connections must run in WAL mode"
    );

    let synchronous: i64 = conn
        .query_row("PRAGMA synchronous;", [], |row| row.get(0))
        .expect("read synchronous");
    assert_eq!(synchronous, 1, "NORMAL synchronous mode is 1");

    let cache_size: i64 = conn
        .query_row("PRAGMA cache_size;", [], |row| row.get(0))
        .expect("read cache_size");
    assert_eq!(cache_size, -64000, "64 MiB page cache");

    let temp_store: i64 = conn
        .query_row("PRAGMA temp_store;", [], |row| row.get(0))
        .expect("read temp_store");
    assert_eq!(temp_store, 2, "temp_store = MEMORY is 2");
}

#[test]
fn test_index_survives_reopen_and_keeps_wal_mode() {
    let dir = tempdir().expect("tempdir");
    let ledger = dir.path().join("ledger.jsonl");
    fs::write(&ledger, "").expect("write empty ledger");
    let sqlite = dir.path().join("index.sqlite");

    {
        let _conn = open_and_sync_index(&sqlite, &ledger).expect("first open");
    }

    let conn = open_and_sync_index(&sqlite, &ledger).expect("reopen");
    let journal_mode: String = conn
        .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
        .expect("read journal_mode");
    assert_eq!(journal_mode.to_lowercase(), "wal");

    let busy_timeout: i64 = conn
        .query_row("PRAGMA busy_timeout;", [], |row| row.get(0))
        .expect("read busy_timeout");
    assert_eq!(busy_timeout, 5000);
}
