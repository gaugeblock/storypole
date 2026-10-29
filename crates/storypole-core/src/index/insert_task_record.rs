//! Projection of one `TaskNode` into the index tables.

use crate::error::StorypoleError;
use crate::model::decision::DecisionStatus;
use crate::model::task::TaskNode;
use rusqlite::{Connection, params};

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
            claim_expires_at, created_at, updated_at, archived_at, raw_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
            task.archived_at_epoch_ms.map(|at| at as i64),
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
