//! Ledger integrity audit command handler.

use std::path::Path;

use crate::json_out::print_json;
use anyhow::{Context, Result};
use storypole_core::ledger::audit::{audit_ledgers, format_ledger_audit};

/// Audits every ledger under `root_dir` and reports structural health plus the replayed statuses.
pub fn handle_ledger_verify(root_dir: &Path, json: bool) -> Result<()> {
    let audit = audit_ledgers(root_dir)
        .with_context(|| format!("Ledger audit failed for {}", root_dir.display()))?;

    if json {
        print_json("ledger-verify", &serde_json::json!({"audit": audit}))?;
    } else {
        print!("{}", format_ledger_audit(&audit));
    }

    if !audit.is_structurally_sound() {
        anyhow::bail!(
            "ledger set is not structurally sound: {} unparsable, {} duplicate",
            audit.unparsable,
            audit.duplicates
        );
    }
    Ok(())
}

/// Compares the unified index against the ledger replay and prints the delta per status.
pub fn handle_ledger_reconcile(root_dir: &Path, json: bool) -> Result<()> {
    let mut audit = audit_ledgers(root_dir)
        .with_context(|| format!("Ledger audit failed for {}", root_dir.display()))?;

    // A key belongs to a repository (ADR 0007), so two repositories owning GB-342 is legal - but the unified index
    // constrains `ticket_key_lower` across the whole suite and refuses to hold both. That surfaced here as a raw
    // `UNIQUE constraint failed` from inside the workspace store, which says nothing about what actually collided
    // (`GB-346`). Name the collisions first: the answer is a decision about identity, not a repaired database.
    if let Some(collisions) = keys_owned_by_several_repositories(root_dir) {
        eprintln!("cross-repository key collisions - the unified index cannot hold these:");
        for (key, repos) in &collisions {
            eprintln!("  {key}: {}", repos.join(", "));
        }
        anyhow::bail!(
            "{} key(s) are owned by more than one repository; GB- is harness only, so each is either a key minted under the wrong prefix or a legacy key awaiting remint - see GB-346 and GB-347",
            collisions.len()
        );
    }

    let workspace = storypole_core::workspace::store::StorypoleWorkspaceStore::open(root_dir)
        .context("Unified index could not be opened")?;
    let filter = storypole_core::index::query::TaskFilter {
        include_archived: true,
        ..Default::default()
    };
    let indexed = storypole_core::index::query::query_tasks(workspace.conn(), &filter)
        .context("Index query failed")?;

    let mut index_counts: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    let mut index_archived = 0usize;
    for task in &indexed {
        if task.archived_at_epoch_ms.is_some() {
            index_archived += 1;
            continue;
        }
        *index_counts
            .entry(format!("{:?}", task.status).to_lowercase())
            .or_insert(0) += 1;
    }
    if index_archived > 0 {
        index_counts.insert("archived".to_string(), index_archived);
    }
    if audit.archived > 0 {
        audit
            .status_counts
            .insert("archived".to_string(), audit.archived);
    }

    let mut statuses: std::collections::BTreeSet<&String> = std::collections::BTreeSet::new();
    statuses.extend(audit.status_counts.keys());
    statuses.extend(index_counts.keys());

    let mut deltas = Vec::new();
    for status in statuses {
        let ledger = audit.status_counts.get(status).copied().unwrap_or(0);
        let index = index_counts.get(status).copied().unwrap_or(0);
        if ledger != index {
            deltas.push((status.clone(), ledger, index));
        }
    }

    if json {
        let payload = serde_json::json!({
            "ledger": audit.status_counts,
            "index": index_counts,
            "deltas": deltas.iter().map(|(s, l, i)| serde_json::json!({"status": s, "ledger": l, "index": i})).collect::<Vec<_>>(),
        });
        print_json(
            "ledger-reconcile",
            &serde_json::json!({"reconcile": payload}),
        )?;
    } else {
        println!(
            "{:<12} {:>8} {:>8} {:>8}",
            "STATUS", "LEDGER", "INDEX", "DELTA"
        );
        for status in [
            "archived",
            "verified",
            "ready",
            "inprogress",
            "deferred",
            "triage",
            "blocked",
            "failed",
        ] {
            let ledger = audit.status_counts.get(status).copied().unwrap_or(0);
            let index = index_counts.get(status).copied().unwrap_or(0);
            if ledger == 0 && index == 0 {
                continue;
            }
            println!(
                "{:<12} {:>8} {:>8} {:>8}",
                status,
                ledger,
                index,
                index as i64 - ledger as i64
            );
        }
    }

    if deltas.is_empty() {
        println!(
            "
index and ledger agree"
        );
        Ok(())
    } else {
        anyhow::bail!(
            "index disagrees with the ledger on {} status group(s) - see GB-177",
            deltas.len()
        )
    }
}

/// Keys that more than one repository owns, with the owners (`GB-346`).
///
/// Returns `None` when every key is unique, which is the state the unified index requires and the state the suite
/// cannot always be in: a key is scoped to its repository, so a collision is legal and this function exists to report
/// it as a collision rather than let it surface as a constraint violation from three layers down.
fn keys_owned_by_several_repositories(root_dir: &Path) -> Option<Vec<(String, Vec<String>)>> {
    let mut owners: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();

    for path in storypole_core::ledger::audit::discover_ticket_files(root_dir) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(document) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(key) = document
            .get("ticket")
            .and_then(|ticket| ticket.get("ticket_key"))
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        // Relative to the root first: the discovered paths are absolute, so the first component of the path itself
        // is `/` for every one of them - which is what this printed before the fix served as its own test.
        let relative = path.strip_prefix(root_dir).unwrap_or(&path);
        let repository = match relative.components().next() {
            Some(first) => {
                let name = first.as_os_str().to_string_lossy().to_string();
                if name.starts_with('.') {
                    ".".to_string()
                } else {
                    name
                }
            }
            None => ".".to_string(),
        };
        owners
            .entry(key.to_uppercase())
            .or_default()
            .push(repository);
    }

    let collisions: Vec<(String, Vec<String>)> = owners
        .into_iter()
        .filter(|(_, repos)| repos.len() > 1)
        .collect();
    if collisions.is_empty() {
        None
    } else {
        Some(collisions)
    }
}
