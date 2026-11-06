//! Handlers for multi-repository workspace commands: list, init, and distribute.

use anyhow::Result;
use std::path::Path;
use storypole_core::config::StorypoleConfig;
use storypole_core::index::query::{TaskFilter, query_tasks};
use storypole_core::ledger::read_events;
use storypole_core::model::LedgerEvent;
use storypole_core::store::StorypoleStore;
use storypole_core::workspace::store::StorypoleWorkspaceStore;

/// Map of canonical suite repository names to their standard ticket prefixes.
pub fn canonical_prefix_for(repo_name: &str) -> &'static str {
    match repo_name {
        "benchwork" => "BW",
        "miter" => "MTR",
        "chalkline" => "CH",
        "dado" => "DA",
        "batten" => "BA",
        "joiner" => "JN",
        "sawhorse" => "SH",
        "outfeed" => "OF",
        "rabbet" => "RB",
        "storypole" => "SP",
        _ => "SP",
    }
}

pub fn handle_workspace_list(dir: &Path, json: bool) -> Result<()> {
    let ws = StorypoleWorkspaceStore::open(dir)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&ws.members())?);
        return Ok(());
    }

    println!("Workspace Root: {}", ws.workspace_root().display());
    println!(
        "{:<14} {:<8} {:<10} {:<30}",
        "MEMBER", "PREFIX", "TASKS", "PATH"
    );
    println!("{}", "-".repeat(70));

    for member in ws.members() {
        let prefix = member.ticket_prefix.as_deref().unwrap_or("-");
        let task_count = if member.dot_dir.is_some() {
            let filter = TaskFilter {
                repo: Some(member.name.clone()),
                ..Default::default()
            };
            query_tasks(ws.conn(), &filter)
                .map(|t| t.len())
                .unwrap_or(0)
        } else {
            0
        };

        println!(
            "{:<14} {:<8} {:<10} {:<30}",
            member.name,
            prefix,
            if member.dot_dir.is_some() {
                task_count.to_string()
            } else {
                "uninit".to_string()
            },
            member.repo_path.display()
        );
    }

    Ok(())
}

/// The verification command a member repository must pass, keyed by repo name (`GB-268`).
///
/// The field is a **lookup map**, not a list of checks: `verification_command_for` resolves it by crate
/// name first, then repo name, and `verify` falls back to a hardcoded `cargo test` when nothing matches.
/// So the shape is one entry per repo holding a single `sh -c` command, chained with `&&` so it stops at
/// the first broken check. An empty map meant every task verified with `cargo test` alone: the gate
/// described in `AGENTS.md` was never wired to this field, which is why it looked present and was absent.
pub(crate) fn default_verification_commands(repo: &str) -> Vec<(String, String)> {
    vec![(
        repo.to_string(),
        format!(
            "cargo fmt --all -- --check && \
             cargo clippy --workspace --all-targets -- -D warnings && \
             cargo nextest run --workspace && \
             chalkline docs check -d {repo}"
        ),
    )]
}

pub fn handle_workspace_init(dir: &Path, json: bool) -> Result<()> {
    let ws = StorypoleWorkspaceStore::open(dir)?;
    let mut initialized = Vec::new();

    for member in ws.members() {
        if member.dot_dir.is_none() {
            let prefix = canonical_prefix_for(&member.name);
            let config = StorypoleConfig {
                ticket_prefix: prefix.to_string(),
                allowed_repos: vec![member.name.clone()],
                claim_ttl_minutes: 60,
                verification_commands: default_verification_commands(&member.name),
                ..Default::default()
            };
            StorypoleStore::init(&member.repo_path, config)?;
            initialized.push((member.name.clone(), prefix));
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&initialized)?);
    } else {
        println!("Initialized {} member repositories:", initialized.len());
        for (name, prefix) in initialized {
            println!("  - {}: prefix {}", name, prefix);
        }
    }

    Ok(())
}

pub fn handle_workspace_distribute(dir: &Path, dry_run: bool, json: bool) -> Result<()> {
    let mut ws = StorypoleWorkspaceStore::open(dir)?;
    let Some(root_store) = ws.root_store_mut() else {
        anyhow::bail!("No root storypole store found to distribute from");
    };

    let events = read_events(&root_store.ledger_path())?;
    let mut tasks_to_distribute = Vec::new();

    for ev in &events {
        if let LedgerEvent::TaskCreated { task } = ev
            && let Some(ref r) = task.scope.repo
            && r != "workspace"
            && r != "gaugeblock"
        {
            tasks_to_distribute.push((r.clone(), task.clone(), ev.clone()));
        }
    }

    if dry_run {
        println!(
            "[DRY RUN] Would distribute {} tasks:",
            tasks_to_distribute.len()
        );
        for (repo, task, _) in &tasks_to_distribute {
            println!("  - {} [{}] -> {}", task.ticket_key, task.title, repo);
        }
        return Ok(());
    }

    // Ensure member stores are initialized
    handle_workspace_init(dir, false)?;
    // Re-open workspace to get active handles
    let mut ws = StorypoleWorkspaceStore::open(dir)?;

    let mut distributed_count = 0;
    for (repo, _task, ev) in tasks_to_distribute {
        if let Some(store) = ws.member_store_mut(&repo) {
            store.record_event(&ev)?;
            distributed_count += 1;
        }
    }

    ws.sync_all_members()?;

    if json {
        println!("{{\"distributed\": {distributed_count}}}");
    } else {
        println!("Successfully distributed {distributed_count} tasks to member stores.");
    }

    Ok(())
}
