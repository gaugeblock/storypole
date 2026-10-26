//! SQLite relational index for fast querying, filtering, and dual ID resolution.

pub mod query;

use crate::error::StorypoleError;
use crate::ledger::read_events;
use crate::ledger::replay::replay_events;
use crate::model::decision::DecisionStatus;
use crate::model::task::TaskNode;
use rusqlite::{Connection, params};
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

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

pub fn init_schema(conn: &Connection) -> Result<(), StorypoleError> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY,
            ticket_key TEXT NOT NULL UNIQUE,
            ticket_key_lower TEXT NOT NULL UNIQUE,
            slug TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL,
            kind TEXT NOT NULL,
            status TEXT NOT NULL,
            priority INTEGER NOT NULL,
            repo TEXT,
            crate_name TEXT,
            module_path TEXT,
            has_pending_decision INTEGER NOT NULL,
            claim_agent TEXT,
            claim_expires_at INTEGER,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            raw_json TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS task_labels (
            task_id TEXT NOT NULL,
            label TEXT NOT NULL,
            PRIMARY KEY (task_id, label)
        );
        CREATE TABLE IF NOT EXISTS task_files (
            task_id TEXT NOT NULL,
            file_path TEXT NOT NULL,
            PRIMARY KEY (task_id, file_path)
        );
        CREATE TABLE IF NOT EXISTS task_symbols (
            task_id TEXT NOT NULL,
            symbol TEXT NOT NULL,
            PRIMARY KEY (task_id, symbol)
        );
        CREATE TABLE IF NOT EXISTS task_edges (
            task_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            target_id TEXT NOT NULL,
            PRIMARY KEY (task_id, kind, target_id)
        );
        CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
        CREATE INDEX IF NOT EXISTS idx_tasks_priority ON tasks(priority);
        CREATE INDEX IF NOT EXISTS idx_tasks_repo_crate ON tasks(repo, crate_name);
        ",
    )
    .map_err(StorypoleError::Sqlite)?;
    Ok(())
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

    match (db_len, db_mtime) {
        (Some(l), Some(m)) if l == ledger_len && m == ledger_mtime => Ok(false),
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
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('ledger_len', ?), ('ledger_mtime', ?)",
        params![ledger_len.to_string(), ledger_mtime.to_string()],
    )
    .map_err(StorypoleError::Sqlite)?;

    tx.commit().map_err(StorypoleError::Sqlite)?;
    Ok(())
}

pub fn insert_task_record(conn: &Connection, task: &TaskNode) -> Result<(), StorypoleError> {
    let has_pending = task
        .decisions
        .iter()
        .any(|d| d.status == DecisionStatus::Pending);
    let kind_str = serde_json::to_string(&task.kind)?.replace('"', "");
    let status_str = serde_json::to_string(&task.status)?.replace('"', "");
    let raw_json = serde_json::to_string(task)?;

    conn.execute(
        "INSERT INTO tasks (
            id, ticket_key, ticket_key_lower, slug, title, kind, status, priority,
            repo, crate_name, module_path, has_pending_decision, claim_agent,
            claim_expires_at, created_at, updated_at, raw_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            task.id.to_string(),
            task.ticket_key,
            task.ticket_key.to_lowercase(),
            task.slug,
            task.title,
            kind_str,
            status_str,
            task.priority as i64,
            task.scope.repo,
            task.scope.crate_name,
            task.scope.module_path,
            if has_pending { 1 } else { 0 },
            task.active_claim.as_ref().map(|c| &c.agent_id),
            task.active_claim
                .as_ref()
                .map(|c| c.lease_expires_at_epoch_ms as i64),
            task.created_at_epoch_ms as i64,
            task.updated_at_epoch_ms as i64,
            raw_json,
        ],
    )
    .map_err(StorypoleError::Sqlite)?;

    for label in &task.scope.labels {
        conn.execute(
            "INSERT OR IGNORE INTO task_labels (task_id, label) VALUES (?, ?)",
            params![task.id.to_string(), label],
        )
        .map_err(StorypoleError::Sqlite)?;
    }

    for file in &task.scope.target_files {
        conn.execute(
            "INSERT OR IGNORE INTO task_files (task_id, file_path) VALUES (?, ?)",
            params![task.id.to_string(), file],
        )
        .map_err(StorypoleError::Sqlite)?;
    }

    for sym in &task.scope.target_symbols {
        conn.execute(
            "INSERT OR IGNORE INTO task_symbols (task_id, symbol) VALUES (?, ?)",
            params![task.id.to_string(), sym],
        )
        .map_err(StorypoleError::Sqlite)?;
    }

    for edge in &task.edges {
        let edge_kind_str = serde_json::to_string(&edge.kind)?.replace('"', "");
        conn.execute(
            "INSERT OR IGNORE INTO task_edges (task_id, kind, target_id) VALUES (?, ?, ?)",
            params![
                task.id.to_string(),
                edge_kind_str,
                edge.target_id.to_string()
            ],
        )
        .map_err(StorypoleError::Sqlite)?;
    }

    Ok(())
}
