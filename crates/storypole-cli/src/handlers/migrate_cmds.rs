//! `storypole migrate` — split one store's tickets into the repositories that own them (`GB-331`).
//!
//! The engine decides and applies; this handler answers the two questions a person has before running it: what
//! will move, and on what evidence? A dry run is the default posture in the sense that it must be asked for
//! explicitly to be destructive - `--dry-run` prints, and without it the migration runs.

use std::path::{Path, PathBuf};

use anyhow::Result;
use storypole_core::migrate::Destination;
use storypole_core::state;
use storypole_core::store::StorypoleStore;
use storypole_core::workspace::discovery::discover_members;

pub fn handle_migrate(root_dir: &Path, dry_run: bool, scope_only: bool, json: bool) -> Result<()> {
    let source = StorypoleStore::resolve_dot_dir(root_dir);
    let loaded = state::load(&source)?;

    let members: Vec<(String, PathBuf)> = discover_members(root_dir, None)
        .into_iter()
        .map(|member| (member.name, member.repo_path))
        .collect();
    let names: Vec<String> = members.iter().map(|(name, _)| name.clone()).collect();
    let owners = storypole_core::migrate::manifests::crate_owners(&members);
    let plan = storypole_core::migrate::plan(&loaded, &names, &|c| owners.get(c).cloned());
    let plan = if scope_only { plan.scope_only() } else { plan };

    if json {
        let rows: Vec<String> = plan
            .assignments
            .iter()
            .map(|assignment| {
                let destination = match &assignment.destination {
                    Destination::Root => "root".to_string(),
                    Destination::Member(repo) => repo.clone(),
                };
                format!(
                    "{{\"key\":\"{}\",\"destination\":\"{destination}\",\"evidence\":\"{}\"}}",
                    assignment.ticket_key, assignment.evidence
                )
            })
            .collect();
        println!(
            "{{\"layout\":\"{:?}\",\"staying\":{},\"moving\":{},\"adopted\":{},\"rows\":[{}]}}",
            loaded.layout,
            plan.staying(),
            plan.assignments.len() - plan.staying(),
            plan.adopted(),
            rows.join(",")
        );
        return Ok(());
    }

    println!(
        "{} ticket(s); {} stay at the root, {} move, {} adopted by crate evidence",
        plan.assignments.len(),
        plan.staying(),
        plan.assignments.len() - plan.staying(),
        plan.adopted()
    );
    println!();
    println!("{:<10} {:<12} {:<10}", "KEY", "DESTINATION", "EVIDENCE");
    println!("{}", "-".repeat(34));
    for assignment in &plan.assignments {
        let destination = match &assignment.destination {
            Destination::Root => "root".to_string(),
            Destination::Member(repo) => repo.clone(),
        };
        println!(
            "{:<10} {:<12} {:<10}",
            assignment.ticket_key, destination, assignment.evidence
        );
    }

    if dry_run {
        println!();
        println!("dry run: nothing was written and the ledger is untouched");
        return Ok(());
    }

    // A destination must be openable afterwards: the config lives at `.gaugeblock/config.toml` or inside the
    // store directory, and a repository with neither - `common` today - would receive tickets it cannot read.
    for repo in plan.per_member().keys() {
        if let Some((_, path)) = members.iter().find(|(name, _)| name == repo) {
            ensure_member_store(path)?;
        }
    }

    let applied = storypole_core::migrate::apply::apply(&source, &loaded, &plan, &|repo| {
        members
            .iter()
            .find(|(name, _)| name == repo)
            .map(|(_, path)| StorypoleStore::resolve_dot_dir(path))
    })?;
    println!();
    println!(
        "migrated {} ticket(s), {} into member repositories; the ledger is preserved at {}",
        applied.written,
        applied.moved,
        applied.archive.display()
    );
    Ok(())
}

/// Creates a member's store when it has none, so the tickets it receives can be opened afterwards.
fn ensure_member_store(member_root: &Path) -> Result<()> {
    let unified = member_root.join(".gaugeblock").join("config.toml");
    let dot = StorypoleStore::resolve_dot_dir(member_root);
    if unified.is_file() || dot.join("config.toml").is_file() {
        return Ok(());
    }
    StorypoleStore::init(
        member_root,
        storypole_core::config::StorypoleConfig::default(),
    )?;
    Ok(())
}
