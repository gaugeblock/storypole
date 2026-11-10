//! `storypole remint` — the mapping step of the suite-wide key migration (`GB-347`).
//!
//! This verb reports and refuses. Reportng the mapping is safe and is what a reviewer needs; applying it means rewriting
//! the reference graph in the same pass, which is a separate step with its own gate. A verb that could half-apply a
//! migration is worse than one that cannot start.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use storypole_core::config::StorypoleConfig;
use storypole_core::ledger::audit::discover_ticket_files;
use storypole_core::ticket_keys::plan_remint;
use storypole_core::tickets::TicketRecord;

use crate::json_out::print_json;

use super::remint::rewrite::cites;

/// Prints the remint mapping for every store under `root_dir`, or refuses to apply it.
///
/// # Errors
///
/// Returns an error when `apply` is requested - applying is a later step - or when a store's configuration cannot be
/// read, because a store whose prefix is unknown cannot be planned safely.
pub fn handle_remint(root_dir: &Path, apply: bool, json: bool) -> Result<()> {
    if apply {
        let applied = super::remint_apply::apply_remint(root_dir)?;
        if json {
            print_json(
                "remint",
                &serde_json::json!({
                    "applied": {
                        "keys": applied.keys,
                        "edges": applied.edges,
                        "citations": applied.citations,
                        "plans": applied.plans,
                        "recorded": applied.recorded,
                    }
                }),
            )?;
        } else {
            println!(
                "{} key(s) reminted, {} edge target(s) rewritten, {} citation(s) rewritten, {} plan file(s) renamed, {} key(s) recorded in {}",
                applied.keys,
                applied.edges,
                applied.citations,
                applied.plans,
                applied.recorded,
                label(root_dir, root_dir)
            );
        }
        return Ok(());
    }

    // Group by store first: a prefix belongs to a repository, so the mapping only means anything per store.
    let mut stores: BTreeMap<PathBuf, Vec<TicketRefs>> = BTreeMap::new();
    for path in discover_ticket_files(root_dir) {
        let Some(refs) = read_refs(&path) else {
            continue;
        };
        stores
            .entry(store_of(root_dir, &path))
            .or_default()
            .push(refs);
    }

    let mut planned = Vec::new();
    let mut total = 0usize;
    let (mut edge_refs, mut body_refs, mut plan_refs) = (0usize, 0usize, 0usize);

    for (store, mut refs) in stores {
        refs.sort_by(|a, b| a.key.cmp(&b.key));
        let prefix = prefix_of(&store).with_context(|| {
            format!(
                "no ticket prefix configured for {} - refusing to guess one",
                store.display()
            )
        })?;
        let keys: Vec<String> = refs.iter().map(|ticket| ticket.key.clone()).collect();
        let mapping = plan_remint(&prefix, &keys);

        // What step 3 would have to repair, counted rather than applied: a key that moves without its references is a
        // graph pointing at nothing, which is why applying is refused until the rewrite exists.
        let moving: std::collections::BTreeSet<&str> =
            mapping.iter().map(|remint| remint.from.as_str()).collect();
        for ticket in &refs {
            edge_refs += ticket
                .edge_targets
                .iter()
                .filter(|target| moving.contains(target.as_str()))
                .count();
            body_refs += moving
                .iter()
                .filter(|key| cites(&ticket.description, key))
                .count();
            if ticket.plan_file.is_some() && moving.contains(ticket.key.as_str()) {
                plan_refs += 1;
            }
        }

        total += mapping.len();
        if !mapping.is_empty() {
            planned.push((store, prefix, keys.len(), mapping));
        }
    }

    if json {
        let payload: Vec<_> = planned
            .iter()
            .map(|(store, prefix, tickets, mapping)| {
                serde_json::json!({
                    "store": store.display().to_string(),
                    "prefix": prefix,
                    "tickets": tickets,
                    "remints": mapping.iter().map(|r| serde_json::json!({"from": r.from, "to": r.to})).collect::<Vec<_>>(),
                })
            })
            .collect();
        print_json("remint", &serde_json::json!({"planned": payload}))?;
        return Ok(());
    }

    for (store, prefix, tickets, mapping) in &planned {
        println!(
            "\n{}  -> prefix {prefix}  ({} tickets, {}/{} off-prefix)",
            label(root_dir, store),
            tickets,
            mapping.len(),
            tickets
        );
        for remint in mapping.iter().take(5) {
            println!("  {} -> {}", remint.from, remint.to);
        }
        if mapping.len() > 5 {
            println!("  ... and {} more", mapping.len() - 5);
        }
    }

    if total == 0 {
        println!("\nnothing to remint: every key already carries its repository's prefix");
    } else {
        println!(
            "\n{total} key(s) would be reminted across {} store(s).",
            planned.len()
        );
        println!(
            "References step 3 must repair alongside them: {edge_refs} edge target(s), {body_refs} body citation(s), {plan_refs} plan file(s)."
        );
        println!("Nothing has been changed.");
    }
    Ok(())
}

/// What one ticket file points at, as far as a remint is concerned (`GB-347` step 3).
pub(crate) struct TicketRefs {
    pub(crate) key: String,
    /// The body, which may cite other keys in prose - read from replay, not the creation snapshot (`GB-349`).
    pub(crate) description: String,
    /// Edge targets, by the key the edge stores.
    pub(crate) edge_targets: Vec<String>,
    /// A plan blueprint named after this ticket, if one exists beside it.
    plan_file: Option<String>,
}

/// Reads the referencing surface of one ticket file, or `None` when it is unreadable.
///
/// The served state comes from **replay**, not from the projection. `ticket` is the task as it was created - later
/// changes live in the events - so counting a remint's citations against it under-reported the work by most of its
/// total, and missed every citation added after creation (`GB-349`).
pub(crate) fn read_refs(path: &Path) -> Option<TicketRefs> {
    let text = std::fs::read_to_string(path).ok()?;
    let record: TicketRecord = serde_json::from_str(&text).ok()?;
    let ticket_key = record.ticket.ticket_key.clone();
    let live = record.replay();
    let description = live.as_ref().map_or_else(
        || record.ticket.description.clone(),
        |task| task.description.clone(),
    );
    let edge_targets: Vec<String> = live.as_ref().map_or_else(
        || {
            record
                .ticket
                .edges
                .iter()
                .filter_map(|edge| edge.target_ticket_key.clone())
                .collect()
        },
        |task| {
            task.edges
                .iter()
                .filter_map(|edge| edge.target_ticket_key.clone())
                .collect()
        },
    );

    // Resolved before the struct literal, because the literal moves `ticket_key` into the struct.
    let plan_file = path
        .parent()
        .and_then(|dir| dir.parent())
        .map(|store| store.join("plans").join(format!("{ticket_key}.md")))
        .filter(|plan| plan.exists())
        .map(|plan| plan.display().to_string());

    Some(TicketRefs {
        key: ticket_key,
        description,
        edge_targets,
        plan_file,
    })
}

/// The repository a ticket file belongs to, as a path from the suite root (`"."` for the root store).
fn store_of(root_dir: &Path, path: &Path) -> PathBuf {
    let relative = path.strip_prefix(root_dir).unwrap_or(path);
    match relative.components().next() {
        Some(first) if !first.as_os_str().to_string_lossy().starts_with('.') => {
            root_dir.join(first)
        }
        _ => root_dir.to_path_buf(),
    }
}

/// The ticket prefix a store declares, from its unified or legacy configuration.
pub(crate) fn prefix_of(store: &Path) -> Result<String> {
    let legacy = store.join(".gaugeblock/storypole/config.toml");
    let config = StorypoleConfig::load_unified_or_legacy(store, &legacy)
        .with_context(|| format!("could not read configuration in {}", store.display()))?;
    Ok(config.ticket_prefix)
}

/// A store's name as a person reads it: relative to the suite root, `"."` for the root's own store.
fn label(root_dir: &Path, store: &Path) -> String {
    match store.strip_prefix(root_dir) {
        Ok(relative) if relative.as_os_str().is_empty() => ".".to_string(),
        Ok(relative) => relative.display().to_string(),
        Err(_) => store.display().to_string(),
    }
}
