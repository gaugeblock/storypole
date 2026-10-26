//! Multi-dimensional query engine and dual ID resolver.

use crate::error::StorypoleError;
use crate::model::task::{TaskKind, TaskNode, TaskStatus};
use rusqlite::{Connection, params};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Multi-dimensional filter criteria for querying tasks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskFilter {
    pub repo: Option<String>,
    pub crate_name: Option<String>,
    pub module_path: Option<String>,
    pub status: Option<TaskStatus>,
    pub kind: Option<TaskKind>,
    pub priority: Option<u8>,
    pub label: Option<String>,
    pub target_file: Option<String>,
    pub target_symbol: Option<String>,
    pub needs_decision: Option<bool>,
    pub exclude_file_collisions: bool,
    pub include_archived: bool,
}

/// Resolve a task by ticket_key (case-insensitive), slug, or UUID full/prefix.
pub fn resolve_task_id(conn: &Connection, identifier: &str) -> Result<Uuid, StorypoleError> {
    let trimmed = identifier.trim();

    // 1. Direct UUID parse
    if let Ok(uid) = Uuid::parse_str(trimmed) {
        return Ok(uid);
    }

    // 2. Case-insensitive ticket_key
    let lower = trimmed.to_lowercase();
    let mut stmt = conn
        .prepare("SELECT id FROM tasks WHERE ticket_key_lower = ?")
        .map_err(StorypoleError::Sqlite)?;
    if let Ok(id_str) = stmt.query_row(params![lower], |r| r.get::<_, String>(0))
        && let Ok(uid) = Uuid::parse_str(&id_str)
    {
        return Ok(uid);
    }

    // 3. Slug match
    let mut stmt = conn
        .prepare("SELECT id FROM tasks WHERE slug = ?")
        .map_err(StorypoleError::Sqlite)?;
    if let Ok(id_str) = stmt.query_row(params![trimmed], |r| r.get::<_, String>(0))
        && let Ok(uid) = Uuid::parse_str(&id_str)
    {
        return Ok(uid);
    }

    // 4. UUID prefix match (>= 6 chars)
    if trimmed.len() >= 6 {
        let mut stmt = conn
            .prepare("SELECT id FROM tasks WHERE id LIKE ? LIMIT 2")
            .map_err(StorypoleError::Sqlite)?;
        let prefix = format!("{trimmed}%");
        let matches: Vec<String> = stmt
            .query_map(params![prefix], |r| r.get::<_, String>(0))
            .map_err(StorypoleError::Sqlite)?
            .filter_map(|r| r.ok())
            .collect();

        if matches.len() == 1
            && let Ok(uid) = Uuid::parse_str(&matches[0])
        {
            return Ok(uid);
        }
    }

    Err(StorypoleError::TaskNotFound(identifier.to_string()))
}

/// Retrieve a single task by its unique UUID.
pub fn get_task_by_id(conn: &Connection, id: Uuid) -> Result<TaskNode, StorypoleError> {
    let mut stmt = conn
        .prepare("SELECT raw_json FROM tasks WHERE id = ?")
        .map_err(StorypoleError::Sqlite)?;

    let json_str: String = stmt
        .query_row(params![id.to_string()], |r| r.get(0))
        .map_err(|_| StorypoleError::TaskNotFound(id.to_string()))?;

    let node: TaskNode = serde_json::from_str(&json_str)?;
    Ok(node)
}

/// Retrieve all tasks in the store indexed by UUID.
pub fn get_all_tasks(conn: &Connection) -> Result<BTreeMap<Uuid, TaskNode>, StorypoleError> {
    let mut stmt = conn
        .prepare("SELECT raw_json FROM tasks")
        .map_err(StorypoleError::Sqlite)?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(StorypoleError::Sqlite)?;

    let mut map = BTreeMap::new();
    for row in rows {
        let json_str = row.map_err(StorypoleError::Sqlite)?;
        let node: TaskNode = serde_json::from_str(&json_str)?;
        map.insert(node.id, node);
    }
    Ok(map)
}

/// Query tasks matching multi-dimensional filter criteria.
pub fn query_tasks(
    conn: &Connection,
    filter: &TaskFilter,
) -> Result<Vec<TaskNode>, StorypoleError> {
    let mut sql = "SELECT DISTINCT t.raw_json FROM tasks t".to_string();
    let mut conditions = Vec::new();
    let mut params_vec: Vec<rusqlite::types::Value> = Vec::new();

    if let Some(ref label) = filter.label {
        sql.push_str(" JOIN task_labels tl ON t.id = tl.task_id");
        conditions.push("tl.label = ?".to_string());
        params_vec.push(label.clone().into());
    }

    if let Some(ref file) = filter.target_file {
        sql.push_str(" JOIN task_files tf ON t.id = tf.task_id");
        conditions.push("tf.file_path = ?".to_string());
        params_vec.push(file.clone().into());
    }

    if let Some(ref sym) = filter.target_symbol {
        sql.push_str(" JOIN task_symbols ts ON t.id = ts.task_id");
        conditions.push("ts.symbol = ?".to_string());
        params_vec.push(sym.clone().into());
    }

    if let Some(ref r) = filter.repo {
        conditions.push("t.repo = ?".to_string());
        params_vec.push(r.clone().into());
    }

    if let Some(ref c) = filter.crate_name {
        conditions.push("t.crate_name = ?".to_string());
        params_vec.push(c.clone().into());
    }

    if let Some(ref m) = filter.module_path {
        conditions.push("t.module_path = ?".to_string());
        params_vec.push(m.clone().into());
    }

    if let Some(st) = filter.status {
        let s = serde_json::to_string(&st)?.replace('"', "");
        conditions.push("t.status = ?".to_string());
        params_vec.push(s.into());
    } else if !filter.include_archived {
        conditions.push("t.status != 'archived'".to_string());
    }

    if let Some(k) = filter.kind {
        let ks = serde_json::to_string(&k)?.replace('"', "");
        conditions.push("t.kind = ?".to_string());
        params_vec.push(ks.into());
    }

    if let Some(p) = filter.priority {
        conditions.push("t.priority = ?".to_string());
        params_vec.push((p as i64).into());
    }

    if let Some(nd) = filter.needs_decision {
        conditions.push("t.has_pending_decision = ?".to_string());
        params_vec.push((if nd { 1i64 } else { 0i64 }).into());
    }

    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }

    sql.push_str(" ORDER BY t.priority ASC, t.created_at ASC");

    let mut stmt = conn.prepare(&sql).map_err(StorypoleError::Sqlite)?;
    let rusqlite_params = rusqlite::params_from_iter(params_vec.iter());
    let rows = stmt
        .query_map(rusqlite_params, |r| r.get::<_, String>(0))
        .map_err(StorypoleError::Sqlite)?;

    let mut result = Vec::new();
    for row in rows {
        let json_str = row.map_err(StorypoleError::Sqlite)?;
        let node: TaskNode = serde_json::from_str(&json_str)?;
        result.push(node);
    }

    Ok(result)
}

/// Compute highest numeric ticket index used for a prefix (e.g. "SP-14" -> 14).
pub fn get_max_ticket_number(conn: &Connection, prefix: &str) -> Result<u64, StorypoleError> {
    let prefix_dash = format!("{}-", prefix.to_lowercase());
    let pattern = format!("{prefix_dash}%");
    let mut stmt = conn
        .prepare("SELECT ticket_key_lower FROM tasks WHERE ticket_key_lower LIKE ?")
        .map_err(StorypoleError::Sqlite)?;
    let rows = stmt
        .query_map([pattern], |r| r.get::<_, String>(0))
        .map_err(StorypoleError::Sqlite)?;

    let mut max_num = 0u64;
    for row in rows {
        let key = row.map_err(StorypoleError::Sqlite)?;
        if let Some(rest) = key.strip_prefix(&prefix_dash)
            && let Ok(num) = rest.parse::<u64>()
            && num > max_num
        {
            max_num = num;
        }
    }
    Ok(max_num)
}
