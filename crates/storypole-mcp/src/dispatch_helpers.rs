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

pub fn handle_set_plan(store: &mut StorypoleStore, args: &Value) -> Result<Value, StorypoleError> {
    use storypole_core::index::query::resolve_task_id;
    use storypole_core::model::plan::{PlanStep, TaskPlan};
    use storypole_core::ops::plan_ops::set_task_plan;

    let ident = req_str(args, "identifier")?;
    let task_id = resolve_task_id(store.conn(), ident)?;
    let summary = req_str(args, "summary")?.to_string();
    let design_markdown = req_str(args, "design_markdown")?.to_string();

    let mut steps = Vec::new();
    if let Some(arr) = args.get("steps").and_then(|v| v.as_array()) {
        for item in arr {
            let step_id = item
                .get("step_id")
                .and_then(|v| v.as_str())
                .unwrap_or("step")
                .to_string();
            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let target_files = opt_vec(item, "target_files");
            let verification_gate = opt_str(item, "verification_gate");
            let completed = opt_bool(item, "completed").unwrap_or(false);
            steps.push(PlanStep {
                step_id,
                title,
                target_files,
                verification_gate,
                completed,
            });
        }
    }

    let plan = TaskPlan {
        summary,
        design_markdown,
        steps,
        plan_file_path: None,
        updated_at_epoch_ms: 0,
    };

    let updated = set_task_plan(store, task_id, plan)?;
    Ok(serde_json::to_value(updated)?)
}
