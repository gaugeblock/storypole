//! Core backlog handlers: init, add, list, show, next, defer, link, split, graph.

use crate::args::*;
use crate::format::*;
use crate::json_out::print_json;
use anyhow::{Result, anyhow};
use std::collections::BTreeMap;
use std::path::Path;
use storypole_core::config::StorypoleConfig;
use storypole_core::dag::briefing::build_task_briefing;
use storypole_core::index::query::{
    TaskFilter, get_all_tasks, get_task_by_id, query_tasks, resolve_task_id,
};
use storypole_core::model::task::{EdgeKind, TaskKind, TaskScope, TaskStatus};
use storypole_core::ops::graph::{defer_issue, link_tasks, split_task};
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::ops::workflow::next_task;
use storypole_core::store::StorypoleStore;
use uuid::Uuid;

pub fn handle_init(dir: &Path, args: InitArgs, json: bool) -> Result<()> {
    let config = StorypoleConfig {
        ticket_prefix: args.prefix,
        claim_ttl_minutes: args.claim_ttl,
        ..Default::default()
    };
    let store = StorypoleStore::init(dir, config)?;
    if json {
        print_json("init", &serde_json::json!({"config": store.config()}))?;
    } else {
        println!(
            "Initialized Storypole store at '{}' (prefix: {})",
            store.dot_dir().display(),
            store.config().ticket_prefix
        );
    }
    Ok(())
}

pub fn handle_add(store: &mut StorypoleStore, args: AddArgs, json: bool) -> Result<()> {
    let kind = parse_kind(&args.kind);
    let scope = TaskScope {
        repo: args.repo,
        crate_name: args.crate_name,
        module_path: args.module,
        target_files: args.files,
        target_symbols: args.symbols,
        labels: args.labels,
    };
    let params = CreateTaskParams {
        title: args.title,
        description: args.desc.unwrap_or_default(),
        kind,
        priority: args.priority,
        scope,
        acceptance_criteria: args.criteria,
        status: None,
        edges: Vec::new(),
    };
    let task = add_task(store, params)?;
    if json {
        print_json("add", &serde_json::json!({"task": task}))?;
    } else {
        println!(
            "Created task {} [{:?}] P{}: {}",
            task.ticket_key, task.kind, task.priority, task.title
        );
    }
    Ok(())
}

pub fn handle_list(store: &StorypoleStore, args: ListArgs, json: bool) -> Result<()> {
    let filter = TaskFilter {
        repo: args.repo,
        crate_name: args.crate_name,
        status: args.status.as_deref().map(parse_status),
        kind: args.kind.as_deref().map(parse_kind),
        label: args.label,
        priority: args.priority,
        include_archived: args.include_archived,
        ..Default::default()
    };
    let tasks = query_tasks(store.conn(), &filter)?;
    if json {
        print_json(
            "list",
            &serde_json::json!({"count": tasks.len(), "tasks": tasks}),
        )?;
    } else {
        print!("{}", format_task_table(&tasks));
    }
    Ok(())
}

pub fn handle_show(store: &StorypoleStore, ident: &str, json: bool) -> Result<()> {
    let id = resolve_task_id(store.conn(), ident)?;
    let task = get_task_by_id(store.conn(), id)?;
    let all_tasks = get_all_tasks(store.conn())?;
    let briefing = build_task_briefing(&task, &all_tasks, store.config(), StorypoleStore::now_ms());
    if json {
        print_json("show", &serde_json::json!({"briefing": briefing}))?;
    } else {
        print!("{}", format_task_briefing(&briefing));
    }
    Ok(())
}

pub fn handle_next(store: &mut StorypoleStore, args: NextArgs, json: bool) -> Result<()> {
    let filter = TaskFilter {
        repo: args.repo,
        crate_name: args.crate_name,
        label: args.label,
        priority: args.priority,
        exclude_file_collisions: args.exclude_file_collisions,
        ..Default::default()
    };
    let briefing_opt = next_task(store, &filter, args.claim.as_deref())?;
    match briefing_opt {
        Some(b) => {
            if json {
                print_json("next", &serde_json::json!({"briefing": b}))?;
            } else {
                print!("{}", format_task_briefing(&b));
            }
        }
        None => {
            if json {
                println!("null");
            } else {
                println!("No ready tasks found matching criteria.");
            }
        }
    }
    Ok(())
}

mod tasks;
pub use tasks::{handle_defer, handle_graph, handle_link, handle_split};
use tasks::{parse_kind, parse_status};
