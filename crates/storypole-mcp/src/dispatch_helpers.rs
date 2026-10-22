//! Argument parsing and validation helpers for MCP tool dispatching.

use serde_json::Value;
use storypole_core::error::StorypoleError;
use storypole_core::model::decision::HandoffPacket;
use storypole_core::model::task::{EdgeKind, TaskKind, TaskScope};
use storypole_core::store::StorypoleStore;

pub fn req_str<'a>(val: &'a Value, key: &str) -> Result<&'a str, StorypoleError> {
    val.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| StorypoleError::InvalidIdentifier(format!("Missing required field '{key}'")))
}

pub fn opt_str(val: &Value, key: &str) -> Option<String> {
    val.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

pub fn opt_u64(val: &Value, key: &str) -> Option<u64> {
    val.get(key).and_then(|v| v.as_u64())
}

pub fn opt_bool(val: &Value, key: &str) -> Option<bool> {
    val.get(key).and_then(|v| v.as_bool())
}

pub fn opt_vec(val: &Value, key: &str) -> Vec<String> {
    val.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn has_scope_fields(val: &Value) -> bool {
    val.get("repo").is_some()
        || val.get("crate_name").is_some()
        || val.get("module_path").is_some()
        || val.get("target_files").is_some()
        || val.get("target_symbols").is_some()
        || val.get("labels").is_some()
}

pub fn parse_scope(val: &Value) -> TaskScope {
    TaskScope {
        repo: opt_str(val, "repo"),
        crate_name: opt_str(val, "crate_name"),
        module_path: opt_str(val, "module_path"),
        target_files: opt_vec(val, "target_files"),
        target_symbols: opt_vec(val, "target_symbols"),
        labels: opt_vec(val, "labels"),
    }
}

pub fn parse_kind(val: Option<&str>) -> TaskKind {
    match val {
        Some("epic") => TaskKind::Epic,
        Some("bug") => TaskKind::Bug,
        Some("refactor") => TaskKind::Refactor,
        Some("tech_debt") => TaskKind::TechDebt,
        Some("spike") => TaskKind::Spike,
        _ => TaskKind::Feature,
    }
}

pub fn parse_edge_kind(val: &str) -> Result<EdgeKind, StorypoleError> {
    match val {
        "blocked_by" => Ok(EdgeKind::BlockedBy),
        "related_to" => Ok(EdgeKind::RelatedTo),
        "parent_epic" => Ok(EdgeKind::ParentEpic),
        "discovered_from" => Ok(EdgeKind::DiscoveredFrom),
        _ => Err(StorypoleError::InvalidIdentifier(format!(
            "Invalid edge kind '{val}'"
        ))),
    }
}

pub fn parse_handoff_packet(val: &Value) -> Result<HandoffPacket, StorypoleError> {
    let agent = req_str(val, "agent_id")?.to_string();
    Ok(HandoffPacket {
        recorded_by_agent: agent,
        timestamp_ms: StorypoleStore::now_ms(),
        completed_items: opt_vec(val, "completed_items"),
        remaining_items: opt_vec(val, "remaining_items"),
        last_error_or_blocker: opt_str(val, "last_error_or_blocker"),
        git_ref: opt_str(val, "git_ref"),
        dirty_files: opt_vec(val, "dirty_files"),
    })
}
