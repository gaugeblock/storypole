//! Building the SQLite index from ticket records (`GB-329`).
//!
//! The ledger path stays: [`super::open_and_sync_index`] is still what a repository without a `tickets/`
//! directory uses. This module is its counterpart, and it keeps the same two properties - the index is
//! disposable, and staleness is decided from the files rather than assumed.

use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use rusqlite::{Connection, params};

use crate::error::StorypoleError;
use crate::ledger::replay::replay_events;
use crate::tickets;

use super::insert_task_record::insert_task_record;
use super::schema::{apply_connection_pragmas, init_schema};

/// Open or rebuild the index from the records under `dot_dir`.
///
/// # Errors
///
/// [`StorypoleError::TicketRecord`] when a record cannot be read, or two records hold one task.
pub fn open_and_sync_records(
    sqlite_path: &Path,
    dot_dir: &Path,
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

    if stored_fingerprint(&conn)? != Some(fingerprint(dot_dir)?) {
        rebuild_from_records(&mut conn, dot_dir)?;
    }
    Ok(conn)
}

/// Replay every record into the index, replacing whatever was there.
///
/// Deleting before inserting is deliberate: the index is a projection, so a rebuild must not depend on
/// what a previous one left behind.
///
/// # Errors
///
/// [`StorypoleError::TicketRecord`] for an unreadable record, [`StorypoleError::Sqlite`] for a failed write.
pub fn rebuild_from_records(
    conn: &mut Connection,
    dot_dir: &Path,
) -> Result<usize, StorypoleError> {
    let records = tickets::read_all(dot_dir)?;
    let mut events = Vec::new();
    for record in &records {
        events.extend(record.events.iter().cloned());
    }
    let tasks = replay_events(&events);

    let tx = conn.transaction().map_err(StorypoleError::Sqlite)?;
    for table in [
        "tasks",
        "task_labels",
        "task_files",
        "task_symbols",
        "task_edges",
    ] {
        tx.execute(&format!("DELETE FROM {table}"), [])
            .map_err(StorypoleError::Sqlite)?;
    }
    for task in tasks.values() {
        insert_task_record(&tx, task)?;
    }
    let (count, digest) = fingerprint(dot_dir)?;
    tx.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('records_count', ?), ('records_digest', ?)",
        params![count.to_string(), digest.to_string()],
    )
    .map_err(StorypoleError::Sqlite)?;
    tx.commit().map_err(StorypoleError::Sqlite)?;
    Ok(tasks.len())
}

/// `(file count, byte-length and mtime digest)` for the tickets directory.
///
/// Cheap to compute and independent of file order, which is all staleness needs: a change to any record,
/// a new record or a deleted one moves it.
fn fingerprint(dot_dir: &Path) -> Result<(i64, i64), StorypoleError> {
    let dir = tickets::dir(dot_dir);
    if !dir.is_dir() {
        return Ok((0, 0));
    }
    let mut count = 0i64;
    let mut digest = 0i64;
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .map_err(|e| StorypoleError::TicketRecord {
            path: dir.clone(),
            reason: e.to_string(),
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    entries.sort();
    for path in entries {
        count += 1;
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        digest = digest
            .wrapping_mul(31)
            .wrapping_add(meta.len() as i64)
            .wrapping_add(mtime);
    }
    Ok((count, digest))
}

/// The fingerprint a previous rebuild recorded, if any.
fn stored_fingerprint(conn: &Connection) -> Result<Option<(i64, i64)>, StorypoleError> {
    let mut stmt = conn
        .prepare("SELECT value FROM meta WHERE key = ?")
        .map_err(StorypoleError::Sqlite)?;
    let mut read = |key: &str| -> Result<Option<i64>, StorypoleError> {
        let value: Option<String> = stmt.query_row(params![key], |row| row.get(0)).ok();
        Ok(value.and_then(|v| v.parse::<i64>().ok()))
    };
    match (read("records_count")?, read("records_digest")?) {
        (Some(count), Some(digest)) => Ok(Some((count, digest))),
        _ => Ok(None),
    }
}
