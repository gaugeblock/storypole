//! `storypole migrate` — split one store's tickets into the repositories that own them (`GB-331`).
//!
//! The engine decides and applies; this handler answers the two questions a person has before running it: what
//! will move, and on what evidence? A dry run is the default posture in the sense that it must be asked for
//! explicitly to be destructive - `--dry-run` prints, and without it the migration runs.

use std::path::{Path, PathBuf};

use anyhow::Result;
use storypole_core::migrate::Destination;
use storypole_core::state;
use storypole_core::storage_mode::StorageMode;
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
    // What applying would write, computed the way `apply` computes it: one record per assignment whose task is
    // still in the state, skipped otherwise. Stated in the dry run because the old output reported only the
    // repository split, so a single-repository store saw "0 moving" and read it as nothing to do while applying
    // wrote all 99 records (`GB-357`).
    let records = plan
        .assignments
        .iter()
        .filter(|assignment| loaded.tasks.contains_key(&assignment.task_id))
        .count();
    let from_layout = loaded.layout;
    let to_layout = StorageMode::Records;
    let archive = source.join("ledger-archive").join("ledger-migrated.jsonl");

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
            "{{\"layout\":\"{from_layout:?}\",\"target\":\"{to_layout:?}\",\"records\":{records},\"staying\":{},\"moving\":{},\"adopted\":{},\"rows\":[{}]}}",
            plan.staying(),
            plan.assignments.len() - plan.staying(),
            plan.adopted(),
            rows.join(",")
        );
        return Ok(());
    }

    match from_layout {
        StorageMode::Ledger => println!(
            "layout: {from_layout:?} -> {to_layout:?} - every ticket becomes a file under tickets/, and the \
             ledger is archived last"
        ),
        StorageMode::Records => println!(
            "layout: {from_layout:?} (already migrated) - applying would rewrite the records and find no ledger \
             to archive"
        ),
    }
    println!(
        "{} of {} ticket(s) would be written as records; {} stay at the root, {} move, {} adopted by crate evidence",
        records,
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
        // The old closing line was "nothing was written and the ledger is untouched", which for a store with
        // zero repository moves read as nothing to do - and that is exactly the store that most needs the
        // layout change. Say what applying would do, including when that is nothing (`GB-357`).
        match (from_layout, records) {
            (StorageMode::Ledger, 0) => println!(
                "dry run: nothing was written. This store holds no tickets, so applying would write nothing \
                 and leave the layout alone."
            ),
            (StorageMode::Ledger, _) => println!(
                "dry run: nothing was written. Applying would write {records} record(s), change the layout \
                 {from_layout:?} -> {to_layout:?} and archive the ledger to {}.",
                archive.display()
            ),
            (StorageMode::Records, 0) => println!(
                "dry run: nothing was written. Already on the {to_layout:?} layout with no tickets to rewrite."
            ),
            (StorageMode::Records, _) => println!(
                "dry run: nothing was written. Applying would rewrite {records} record(s) in place; this store is \
                 already on the {to_layout:?} layout, so there is no ledger to archive."
            ),
        }
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
