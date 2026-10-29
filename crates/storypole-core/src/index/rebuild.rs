//! Deciding when the index is stale, and rebuilding it from the ledger.

use crate::error::StorypoleError;
use crate::ledger::read_events;
use crate::ledger::replay::replay_events;
use rusqlite::{Connection, params};
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use super::insert_task_record::insert_task_record;
use super::schema::{apply_connection_pragmas, init_schema};

/// Semantics version of the replay. Bump when a change alters how events are interpreted, which
/// no ledger edit would signal on its own.
const REPLAY_VERSION: &str = "2";

/// Open or synchronize the ephemeral SQLite database against `ledger.jsonl`.
pub fn open_and_sync_index(
    sqlite_path: &Path,
    ledger_path: &Path,
) -> Result<Connection, StorypoleError> {
    if let Some(parent) = sqlite_path.parent() {
        fs::create_dir_all(parent).map_err(|e| StorypoleError::IoWithPath {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }

    let mut conn = Connection::open(sqlite_path).map_err(StorypoleError::Sqlite)?;
    apply_connection_pragmas(&conn)?;
    init_schema(&conn)?;

    let ledger_meta = fs::metadata(ledger_path).ok();
    let ledger_len = ledger_meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let ledger_mtime = ledger_meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let needs_rebuild = check_needs_rebuild(&conn, ledger_len as i64, ledger_mtime)?;
    if needs_rebuild {
        rebuild_index(&mut conn, ledger_path, ledger_len as i64, ledger_mtime)?;
    }

    Ok(conn)
}

fn check_needs_rebuild(
    conn: &Connection,
    ledger_len: i64,
    ledger_mtime: i64,
) -> Result<bool, StorypoleError> {
    let mut stmt = conn
        .prepare("SELECT value FROM meta WHERE key = ?")
        .map_err(StorypoleError::Sqlite)?;

    let db_len: Option<i64> = stmt
        .query_row(params!["ledger_len"], |r| r.get::<_, String>(0))
        .ok()
        .and_then(|v| v.parse().ok());

    let db_mtime: Option<i64> = stmt
        .query_row(params!["ledger_mtime"], |r| r.get::<_, String>(0))
        .ok()
        .and_then(|v| v.parse().ok());

    let db_replay_version: Option<String> = stmt
        .query_row(params!["replay_version"], |r| r.get::<_, String>(0))
        .ok();

    // A replay change alters what the ledger means without touching the file, so the version takes
    // part in the cache key. Archiving becoming a state (GB-179) is exactly that case.
    let version_matches = db_replay_version.as_deref() == Some(REPLAY_VERSION);

    match (db_len, db_mtime) {
        (Some(l), Some(m)) if version_matches && l == ledger_len && m == ledger_mtime => Ok(false),
        _ => Ok(true),
    }
}

/// Force rebuild of index tables from `ledger.jsonl`.
pub fn rebuild_index(
    conn: &mut Connection,
    ledger_path: &Path,
    ledger_len: i64,
    ledger_mtime: i64,
) -> Result<(), StorypoleError> {
    let events = read_events(ledger_path)?;
    let tasks = replay_events(&events);

    let tx = conn.transaction().map_err(StorypoleError::Sqlite)?;

    tx.execute("DELETE FROM task_labels", [])
        .map_err(StorypoleError::Sqlite)?;
    tx.execute("DELETE FROM task_files", [])
        .map_err(StorypoleError::Sqlite)?;
    tx.execute("DELETE FROM task_symbols", [])
        .map_err(StorypoleError::Sqlite)?;
    tx.execute("DELETE FROM task_edges", [])
        .map_err(StorypoleError::Sqlite)?;
    tx.execute("DELETE FROM tasks", [])
        .map_err(StorypoleError::Sqlite)?;

    for task in tasks.values() {
        insert_task_record(&tx, task)?;
    }

    tx.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('ledger_len', ?), ('ledger_mtime', ?), ('replay_version', ?)",
        params![
            ledger_len.to_string(),
            ledger_mtime.to_string(),
            REPLAY_VERSION.to_string()
        ],
    )
    .map_err(StorypoleError::Sqlite)?;

    tx.commit().map_err(StorypoleError::Sqlite)?;
    Ok(())
}
