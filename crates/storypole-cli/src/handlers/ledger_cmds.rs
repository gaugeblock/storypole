//! Ledger integrity audit command handler.

use std::path::Path;

use anyhow::{Context, Result};
use storypole_core::ledger::audit::{audit_ledgers, format_ledger_audit};

/// Audits every ledger under `root_dir` and reports structural health plus the replayed statuses.
pub fn handle_ledger_verify(root_dir: &Path, json: bool) -> Result<()> {
    let audit = audit_ledgers(root_dir)
        .with_context(|| format!("Ledger audit failed for {}", root_dir.display()))?;

    if json {
        println!("{}", serde_json::to_string_pretty(&audit)?);
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
    let audit = audit_ledgers(root_dir)
        .with_context(|| format!("Ledger audit failed for {}", root_dir.display()))?;

    let workspace = storypole_core::workspace::store::StorypoleWorkspaceStore::open(root_dir)
        .context("Unified index could not be opened")?;
    let filter = storypole_core::index::query::TaskFilter::default();
    let indexed = storypole_core::index::query::query_tasks(workspace.conn(), &filter)
        .context("Index query failed")?;

    let mut index_counts: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for task in &indexed {
        *index_counts
            .entry(format!("{:?}", task.status).to_lowercase())
            .or_insert(0) += 1;
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
        println!("{}", serde_json::to_string_pretty(&payload)?);
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
