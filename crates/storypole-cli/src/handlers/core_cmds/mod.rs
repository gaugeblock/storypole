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

    // An all-archived store printed exactly what an empty store prints, which read as a broken store during `GB-347`.
    // Counting what the filter hid costs one more query and answers the question the empty table raises.
    let hidden = if args.include_archived {
        0
    } else {
        let mut widened = filter.clone();
        widened.include_archived = true;
        query_tasks(store.conn(), &widened)?
            .len()
            .saturating_sub(tasks.len())
    };

    if json {
        print_json(
            "list",
            &serde_json::json!({"count": tasks.len(), "archived_hidden": hidden, "tasks": tasks}),
        )?;
    } else {
        print!("{}", format_task_table(&tasks));
        if hidden > 0 {
            println!(
                "{hidden} archived ticket(s) hidden by this view; --include-archived lists them"
            );
        }
    }
    Ok(())
}

pub fn handle_show(store: &StorypoleStore, ident: &str, json: bool) -> Result<()> {
    let id = match resolve_task_id(store.conn(), ident) {
        Ok(id) => id,
        // A ticket lives in exactly one repository since ADR 0009, so "not found" here may simply mean the
        // caller is in the wrong directory. Saying which directory is the difference between a dead end and an
        // instruction (`GB-330`).
        Err(error) => {
            match storypole_core::workspace::lookup::locate_ticket(store.root_dir(), ident) {
                Some(dot) => {
                    // `<repo>/.gaugeblock/storypole`: counting from the store, the checkout is `nth(2)`.
                    let repo = dot.ancestors().nth(2).unwrap_or(&dot);
                    anyhow::bail!(
                        "{ident} belongs to the store at {}; re-run with --dir {}",
                        dot.display(),
                        repo.display()
                    )
                }
                None => return Err(error.into()),
            }
        }
    };
    let task = get_task_by_id(store.conn(), id)?;
    let all_tasks = get_all_tasks(store.conn())?;
    let briefing = build_task_briefing(&task, &all_tasks, store.config(), StorypoleStore::now_ms());
    if json {
        print_json(
            "show",
            &serde_json::json!({
                "briefing": briefing,
                // Non-null when the receipt names commits `main` never received: the ledger's strongest
                // claim pointing at work that exists nowhere else (`GB-325`).
                "unmerged_receipt": storypole_core::ops::revision::unmerged_receipt_warning(
                    store.root_dir(),
                    &task,
                ),
            }),
        )?;
    } else {
        print!("{}", format_task_briefing(&briefing));
        if let Some(warning) =
            storypole_core::ops::revision::unmerged_receipt_warning(store.root_dir(), &task)
        {
            eprintln!("{warning}");
        }
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
