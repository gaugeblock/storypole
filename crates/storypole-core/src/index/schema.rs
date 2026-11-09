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
            -- identity: the target's uuid. The two columns below are the locator, and they exist because a
            -- cross-repository edge cannot be rendered from a uuid in a checkout that does not hold the
            -- target (`GB-332`).
            target_id TEXT NOT NULL,
            target_ticket_key TEXT,
            target_repo TEXT,
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

    ensure_edge_locator_columns(conn)?;
    Ok(())
}

/// Adds the locator columns to an index built before they existed.
///
/// `CREATE TABLE IF NOT EXISTS` does not alter an existing table, so an index created by an earlier version keeps
/// its three columns and silently drops the locator on every rebuild. `ALTER TABLE ... ADD COLUMN` is additive and
/// safe to attempt on every open.
///
/// # Errors
///
/// [`StorypoleError::Sqlite`] when the table cannot be inspected or altered.
fn ensure_edge_locator_columns(conn: &rusqlite::Connection) -> Result<(), StorypoleError> {
    for column in ["target_ticket_key", "target_repo"] {
        if !has_column(conn, "task_edges", column)? {
            conn.execute(
                &format!("ALTER TABLE task_edges ADD COLUMN {column} TEXT"),
                [],
            )
            .map_err(StorypoleError::Sqlite)?;
        }
    }
    Ok(())
}

/// Whether a table already has a column.
fn has_column(
    conn: &rusqlite::Connection,
    table: &str,
    column: &str,
) -> Result<bool, StorypoleError> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(StorypoleError::Sqlite)?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(StorypoleError::Sqlite)?;
    for name in names {
        if name.map_err(StorypoleError::Sqlite)? == column {
            return Ok(true);
        }
    }
    Ok(false)
}
