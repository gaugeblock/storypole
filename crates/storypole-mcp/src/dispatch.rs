//! Dispatch handlers mapping MCP tool calls to `storypole-core` operations.

use crate::dispatch_helpers::*;
use serde_json::{Value, json};
use storypole_core::dag::briefing::build_task_briefing;
use storypole_core::error::StorypoleError;
use storypole_core::index::query::{TaskFilter, get_all_tasks, get_task_by_id, resolve_task_id};
use storypole_core::model::task::TaskStatus;
use storypole_core::ops::decisions::{ask_decision, decide, supersede_decision};
use storypole_core::ops::graph::{defer_issue, link_tasks, split_task};
use storypole_core::ops::mutations::{CreateTaskParams, UpdateTaskParams, add_task, update_task};
use storypole_core::ops::workflow::{claim_task, handoff_task, heartbeat, next_task, verify_task};
use storypole_core::store::StorypoleStore;
use uuid::Uuid;

/// Execute an MCP tool invocation and return a JSON value payload.
pub fn dispatch_tool(
    name: &str,
    args: &Value,
    store: &mut StorypoleStore,
) -> Result<Value, StorypoleError> {
    match name {
        "storypole_next" => {
            let filter = TaskFilter {
                repo: opt_str(args, "repo"),
                crate_name: opt_str(args, "crate_name"),
                module_path: opt_str(args, "module_path"),
                label: opt_str(args, "label"),
                priority: opt_u64(args, "priority").map(|p| p as u8),
                exclude_file_collisions: opt_bool(args, "exclude_file_collisions").unwrap_or(false),
                ..Default::default()
            };
            let auto_claim = opt_str(args, "auto_claim_agent");
            let briefing = next_task(store, &filter, auto_claim.as_deref())?;
            Ok(json!(briefing))
        }
        "storypole_show" => {
            let ident = req_str(args, "identifier")?;
            let task_id = resolve_task_id(store.conn(), ident)?;
            let task = get_task_by_id(store.conn(), task_id)?;
            let all_tasks = get_all_tasks(store.conn())?;
            let briefing =
                build_task_briefing(&task, &all_tasks, store.config(), StorypoleStore::now_ms());
            Ok(json!(briefing))
        }
        "storypole_add" => {
            let title = req_str(args, "title")?.to_string();
            let kind = parse_kind(opt_str(args, "kind").as_deref());
            let priority = opt_u64(args, "priority").unwrap_or(3) as u8;
            let scope = parse_scope(args);
            let acceptance_criteria = opt_vec(args, "acceptance_criteria");
            let params = CreateTaskParams {
                title,
                description: opt_str(args, "description").unwrap_or_default(),
                kind,
                priority,
                scope,
                acceptance_criteria,
                status: None,
                edges: Vec::new(),
            };
            let task = add_task(store, params)?;
            Ok(json!(task))
        }
        "storypole_update" => {
            let ident = req_str(args, "identifier")?;
            let task_id = resolve_task_id(store.conn(), ident)?;
            let params = UpdateTaskParams {
                title: opt_str(args, "title"),
                description: opt_str(args, "description"),
                priority: opt_u64(args, "priority").map(|p| p as u8),
                scope: if has_scope_fields(args) {
                    Some(parse_scope(args))
                } else {
                    None
                },
                acceptance_criteria: if args.get("acceptance_criteria").is_some() {
                    Some(opt_vec(args, "acceptance_criteria"))
                } else {
                    None
                },
            };
            let task = update_task(store, task_id, params)?;
            Ok(json!(task))
        }
        "storypole_link" => {
            let src = req_str(args, "source")?;
            let tgt = req_str(args, "target")?;
            let k_str = req_str(args, "kind")?;
            let kind = parse_edge_kind(k_str)?;
            let src_id = resolve_task_id(store.conn(), src)?;
            let tgt_id = resolve_task_id(store.conn(), tgt)?;
            link_tasks(store, src_id, tgt_id, kind)?;
            Ok(json!({ "success": true, "source": src, "target": tgt, "kind": k_str }))
        }
        "storypole_defer" => {
            let title = req_str(args, "title")?.to_string();
            let captured_by = req_str(args, "captured_by")?.to_string();
            let kind = parse_kind(opt_str(args, "kind").as_deref());
            let from_id = match opt_str(args, "discovered_from") {
                Some(ref s) => Some(resolve_task_id(store.conn(), s)?),
                None => None,
            };
            let scope = parse_scope(args);
            let task = defer_issue(
                store,
                title,
                opt_str(args, "description").unwrap_or_default(),
                kind,
                scope,
                from_id,
                captured_by,
            )?;
            Ok(json!(task))
        }
        "storypole_split" => {
            let ident = req_str(args, "identifier")?;
            let task_id = resolve_task_id(store.conn(), ident)?;
            let child_titles = opt_vec(args, "child_titles");
            let seq = opt_bool(args, "sequential").unwrap_or(true);
            let children = split_task(store, task_id, child_titles, seq)?;
            Ok(json!(children))
        }
        "storypole_ask_decision" => {
            let ident = req_str(args, "identifier")?;
            let task_id = resolve_task_id(store.conn(), ident)?;
            let cat = opt_str(args, "category").unwrap_or_else(|| "general".into());
            let question = req_str(args, "question")?.to_string();
            let options = opt_vec(args, "proposed_options");
            let parent_id = match opt_str(args, "parent_decision_id") {
                Some(s) => Some(Uuid::parse_str(&s).map_err(|e| {
                    StorypoleError::InvalidIdentifier(format!("parent_decision_id: {e}"))
                })?),
                None => None,
            };
            let dec_id = ask_decision(store, task_id, cat, question, options, parent_id)?;
            Ok(json!({ "decision_id": dec_id }))
        }
        "storypole_decide" => {
            let ident = req_str(args, "identifier")?;
            let task_id = resolve_task_id(store.conn(), ident)?;
            let d_str = req_str(args, "decision_id")?;
            let dec_id = Uuid::parse_str(d_str)
                .map_err(|e| StorypoleError::InvalidIdentifier(format!("decision_id: {e}")))?;
            let chosen = req_str(args, "chosen_option")?.to_string();
            let rationale = req_str(args, "rationale")?.to_string();
            let resolved_by = req_str(args, "resolved_by")?.to_string();
            let crit = opt_str(args, "appended_criterion");
            decide(
                store,
                task_id,
                dec_id,
                chosen,
                rationale,
                resolved_by,
                None,
                crit,
            )?;
            Ok(json!({ "success": true, "decision_id": d_str }))
        }
        "storypole_supersede_decision" => {
            let ident = req_str(args, "identifier")?;
            let task_id = resolve_task_id(store.conn(), ident)?;
            let d_str = req_str(args, "decision_id")?;
            let dec_id = Uuid::parse_str(d_str)
                .map_err(|e| StorypoleError::InvalidIdentifier(format!("decision_id: {e}")))?;
            let reason = req_str(args, "reason")?.to_string();
            supersede_decision(store, task_id, dec_id, reason)?;
            Ok(json!({ "success": true, "decision_id": d_str, "status": "superseded" }))
        }
        "storypole_claim" => {
            let ident = req_str(args, "identifier")?;
            let task_id = resolve_task_id(store.conn(), ident)?;
            let agent = req_str(args, "agent_id")?;
            let branch = opt_str(args, "branch");
            let briefing = claim_task(store, task_id, agent, branch.as_deref())?;
            Ok(json!(briefing))
        }
        "storypole_set_plan" => handle_set_plan(store, args),
        "storypole_heartbeat" => {
            let ident = req_str(args, "identifier")?;
            let task_id = resolve_task_id(store.conn(), ident)?;
            let agent = req_str(args, "agent_id")?;
            let expires_at = heartbeat(store, task_id, agent)?;
            Ok(json!({ "lease_expires_at_epoch_ms": expires_at }))
        }
        "storypole_handoff" => {
            let ident = req_str(args, "identifier")?;
            let task_id = resolve_task_id(store.conn(), ident)?;
            let packet = parse_handoff_packet(args)?;
            let status = match opt_str(args, "target_status").as_deref() {
                Some("ready") => TaskStatus::Ready,
                Some("failed") => TaskStatus::Failed,
                _ => TaskStatus::Blocked,
            };
            handoff_task(store, task_id, packet, status)?;
            Ok(json!({ "success": true, "status": format!("{status:?}") }))
        }
        "storypole_verify" => {
            let ident = req_str(args, "identifier")?;
            let task_id = resolve_task_id(store.conn(), ident)?;
            let run_cmd = opt_bool(args, "run_command").unwrap_or(false);
            let receipt = verify_task(store, task_id, None, run_cmd)?;
            Ok(json!(receipt))
        }
        _ => Err(StorypoleError::InvalidIdentifier(format!(
            "Unknown MCP tool '{name}'"
        ))),
    }
}
