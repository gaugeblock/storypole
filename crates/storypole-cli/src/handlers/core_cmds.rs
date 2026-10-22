//! Core backlog handlers: init, add, list, show, next, defer, link, split, graph.

use crate::args::*;
use crate::format::*;
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
        println!("{}", serde_json::to_string_pretty(store.config())?);
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
        println!("{}", serde_json::to_string_pretty(&task)?);
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
        ..Default::default()
    };
    let tasks = query_tasks(store.conn(), &filter)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&tasks)?);
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
        println!("{}", serde_json::to_string_pretty(&briefing)?);
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
                println!("{}", serde_json::to_string_pretty(&b)?);
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

pub fn handle_defer(store: &mut StorypoleStore, args: DeferArgs, json: bool) -> Result<()> {
    let kind = parse_kind(&args.kind);
    let from_id = match args.from {
        Some(ref f) => Some(resolve_task_id(store.conn(), f)?),
        None => None,
    };
    let scope = TaskScope {
        repo: args.repo,
        crate_name: args.crate_name,
        target_files: args.files,
        target_symbols: args.symbols,
        labels: args.labels,
        ..Default::default()
    };
    let task = defer_issue(
        store,
        args.title,
        args.desc.unwrap_or_default(),
        kind,
        scope,
        from_id,
        args.captured_by,
    )?;
    if json {
        println!("{}", serde_json::to_string_pretty(&task)?);
    } else {
        println!(
            "Deferred task captured: {} [{:?}] {}",
            task.ticket_key, task.status, task.title
        );
    }
    Ok(())
}

pub fn handle_link(
    store: &mut StorypoleStore,
    src: &str,
    tgt: &str,
    kind_str: &str,
    json: bool,
) -> Result<()> {
    let src_id = resolve_task_id(store.conn(), src)?;
    let tgt_id = resolve_task_id(store.conn(), tgt)?;
    let kind = parse_edge_kind(kind_str)?;
    link_tasks(store, src_id, tgt_id, kind)?;
    if json {
        println!("{{\"success\":true}}");
    } else {
        println!("Linked {src} -> {tgt} ({kind_str})");
    }
    Ok(())
}

pub fn handle_split(store: &mut StorypoleStore, args: SplitArgs, json: bool) -> Result<()> {
    let id = resolve_task_id(store.conn(), &args.identifier)?;
    let children = split_task(store, id, args.child_titles, args.sequential)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&children)?);
    } else {
        println!(
            "Split task {} into {} subtasks:",
            args.identifier,
            children.len()
        );
        for c in children {
            println!("  * {} [{:?}]: {}", c.ticket_key, c.status, c.title);
        }
    }
    Ok(())
}

pub fn handle_graph(store: &StorypoleStore, _format: &str, json: bool) -> Result<()> {
    let tasks: BTreeMap<Uuid, _> = get_all_tasks(store.conn())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&tasks)?);
    } else {
        print!("{}", format_mermaid_graph(&tasks));
    }
    Ok(())
}

fn parse_kind(s: &str) -> TaskKind {
    match s.to_lowercase().as_str() {
        "epic" => TaskKind::Epic,
        "bug" => TaskKind::Bug,
        "refactor" => TaskKind::Refactor,
        "tech_debt" | "techdebt" => TaskKind::TechDebt,
        "spike" => TaskKind::Spike,
        _ => TaskKind::Feature,
    }
}

fn parse_status(s: &str) -> TaskStatus {
    match s.to_lowercase().as_str() {
        "blocked" => TaskStatus::Blocked,
        "ready" => TaskStatus::Ready,
        "in_progress" | "inprogress" => TaskStatus::InProgress,
        "verified" => TaskStatus::Verified,
        "deferred" => TaskStatus::Deferred,
        "failed" => TaskStatus::Failed,
        _ => TaskStatus::Triage,
    }
}

fn parse_edge_kind(s: &str) -> Result<EdgeKind> {
    match s.to_lowercase().as_str() {
        "blocked_by" | "blockedby" => Ok(EdgeKind::BlockedBy),
        "related_to" | "relatedto" => Ok(EdgeKind::RelatedTo),
        "parent_epic" | "parentepic" => Ok(EdgeKind::ParentEpic),
        "discovered_from" | "discoveredfrom" => Ok(EdgeKind::DiscoveredFrom),
        other => Err(anyhow!("Unknown edge kind '{other}'")),
    }
}
