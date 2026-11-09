//! Task structure commands: defer, link, split.
//!
//! Split from the creation and query handlers when the file passed the 250-line limit (`GB-291`).

use super::*;
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
        print_json("defer", &serde_json::json!({"task": task}))?;
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
        print_json("split", &serde_json::json!({"children": children}))?;
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
        print_json(
            "graph",
            &serde_json::json!({"count": tasks.len(), "tasks": tasks}),
        )?;
    } else {
        print!("{}", format_mermaid_graph(&tasks));
    }
    Ok(())
}

pub(super) fn parse_kind(s: &str) -> TaskKind {
    match s.to_lowercase().as_str() {
        "epic" => TaskKind::Epic,
        "bug" => TaskKind::Bug,
        "refactor" => TaskKind::Refactor,
        "tech_debt" | "techdebt" | "tech-debt" => TaskKind::TechDebt,
        "chore" => TaskKind::Chore,
        "docs" => TaskKind::Docs,
        "spike" => TaskKind::Spike,
        _ => TaskKind::Feature,
    }
}

pub(super) fn parse_status(s: &str) -> TaskStatus {
    match s.to_lowercase().as_str() {
        "blocked" => TaskStatus::Blocked,
        "ready" => TaskStatus::Ready,
        "in_progress" | "inprogress" => TaskStatus::InProgress,
        "verified" => TaskStatus::Verified,
        "archived" => TaskStatus::Archived,
        "deferred" => TaskStatus::Deferred,
        "failed" => TaskStatus::Failed,
        _ => TaskStatus::Triage,
    }
}

pub(super) fn parse_edge_kind(s: &str) -> Result<EdgeKind> {
    match s.to_lowercase().as_str() {
        "blocked_by" | "blockedby" => Ok(EdgeKind::BlockedBy),
        "related_to" | "relatedto" => Ok(EdgeKind::RelatedTo),
        "parent_epic" | "parentepic" => Ok(EdgeKind::ParentEpic),
        "discovered_from" | "discoveredfrom" => Ok(EdgeKind::DiscoveredFrom),
        other => Err(anyhow!("Unknown edge kind '{other}'")),
    }
}
