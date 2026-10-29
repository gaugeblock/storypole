//! Index connection pragmas and table schema.

use crate::error::StorypoleError;
use rusqlite::Connection;

/// Performance PRAGMAs applied to every index connection.
///
/// WAL keeps readers from blocking the writer during ledger replays, `synchronous = NORMAL`
/// removes the second `fsync()` per transaction that the default `FULL` mode issues, and the
/// in-memory temp store plus a 64 MiB page cache keep rebuilds off the disk. `busy_timeout`
/// lets concurrent processes (CLI, MCP server, benchwork daemon) wait instead of erroring.
const INDEX_PRAGMAS: &str = "
    PRAGMA journal_mode = WAL;
    PRAGMA synchronous = NORMAL;
    PRAGMA temp_store = MEMORY;
    PRAGMA cache_size = -64000;
    PRAGMA busy_timeout = 5000;
";

/// Applies the index performance PRAGMAs ([`INDEX_PRAGMAS`]) to an open connection.
pub fn apply_connection_pragmas(conn: &Connection) -> Result<(), StorypoleError> {
    conn.execute_batch(INDEX_PRAGMAS)
        .map_err(StorypoleError::Sqlite)
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
            archived_at INTEGER,
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
    // Indexes created before archiving became a state lack the column; a failed ALTER means
    // it is already present (GB-179).
    let _ = conn.execute("ALTER TABLE tasks ADD COLUMN archived_at INTEGER", []);

    Ok(())
}
