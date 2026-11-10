//! Applying a remint to one store (`GB-347` step 3).
//!
//! One store at a time, deliberately: applying across eleven repositories in one command would be a single
//! irreversible step with nowhere to bisect.
//!
//! Keys, edge targets, citations and plan filenames move in the same pass, because a key that moves without its
//! references leaves a live graph pointing at nothing. The rewriting itself lives in [`super::remint`]; this module
//! decides what moves and in what order.
//!
//! The pass is verified twice, and the second check is the one that matters. "Every key carries its prefix" is a
//! property of the file; "replay serves no moved key" is a property of the store. The first version of this verb proved
//! the former while leaving the latter false in nine stores (`GB-347`).

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;
use storypole_core::ticket_keys::plan_remint;
use uuid::Uuid;

use super::remint::events::rewrite_event;
use super::remint::record::{self, AppliedRemint};
use super::remint::rewrite::{Rewrite, rewrite_task};
use super::remint::verify::verify_replay;
use super::remint_cmds::{prefix_of, read_refs};
use super::remint_store::{rename_plan, store_tickets};

/// What applying a remint did, for reporting and for tests.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Applied {
    /// Keys renamed.
    pub keys: usize,
    /// Edge targets rewritten.
    pub edges: usize,
    /// Body citations rewritten.
    pub citations: usize,
    /// Plan blueprints renamed alongside their ticket.
    pub plans: usize,
    /// Keys written to the store's remint record, so a later pass can resolve a reference to them.
    pub recorded: usize,
}

/// Renames every off-prefix key in the store at `store`, rewriting what points at them.
///
/// # Errors
///
/// Returns an error when the store's prefix is unknown, when a ticket file cannot be read or written, when any key is
/// still off-prefix afterwards, or when the replayed state of a ticket still cites a key that moved.
pub fn apply_remint(store: &Path) -> Result<Applied> {
    let prefix = prefix_of(store)?;
    let mut files = store_tickets(store);
    files.sort();
    if files.is_empty() {
        anyhow::bail!("no tickets in {}", store.display());
    }

    let keys: Vec<String> = files
        .iter()
        .filter_map(|path| read_refs(path).map(|refs| refs.key))
        .collect();
    let mapping = plan_remint(&prefix, &keys);
    if mapping.is_empty() {
        return Ok(Applied::default());
    }
    let lookup: BTreeMap<String, String> = mapping
        .iter()
        .map(|remint| (remint.from.clone(), remint.to.clone()))
        .collect();

    let mut applied = Applied::default();
    let mut history: Vec<AppliedRemint> = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("could not read {}", path.display()))?;
        let mut document: Value = serde_json::from_str(&text)
            .with_context(|| format!("{} is not valid JSON", path.display()))?;

        let old_key = document
            .get("ticket")
            .and_then(|ticket| ticket.get("ticket_key"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let new_key = lookup.get(&old_key).cloned();

        // Recorded before the file moves: the uuid is what a later cross-store pass resolves a reference against.
        if let (Some(new_key), Some(id)) = (
            new_key.as_ref(),
            document
                .get("ticket")
                .and_then(|ticket| ticket.get("id"))
                .and_then(Value::as_str)
                .and_then(|text| Uuid::parse_str(text).ok()),
        ) {
            history.push(AppliedRemint {
                id,
                from: old_key.clone(),
                to: new_key.clone(),
            });
        }

        if let Some(ticket) = document.get_mut("ticket") {
            if let Some(new_key) = &new_key {
                ticket["ticket_key"] = Value::String(new_key.clone());
                applied.keys += 1;
            }
            let (edges, citations) = rewrite_task(ticket, &lookup, &mapping, Rewrite::REMINT);
            applied.edges += edges;
            applied.citations += citations;
        }

        // The event log carries a full snapshot per event and the index is built by replaying it, so it is live state
        // rather than history. It is also where replay reads fields the projection only appears to hold - see
        // `super::remint::events` for which, and why rewriting the projection alone still passed verification.
        if let Some(events) = document.get_mut("events").and_then(Value::as_array_mut) {
            for snapshot in events {
                let (edges, citations) =
                    rewrite_event(snapshot, &lookup, &mapping, Rewrite::REMINT);
                applied.edges += edges;
                applied.citations += citations;
            }
        }

        let target = match &new_key {
            Some(new_key) => path.with_file_name(format!("{new_key}.json")),
            None => path.clone(),
        };
        std::fs::write(&target, serde_json::to_string_pretty(&document)?)
            .with_context(|| format!("could not write {}", target.display()))?;
        if target != *path {
            std::fs::remove_file(path)
                .with_context(|| format!("could not remove {}", path.display()))?;
        }

        if let Some(new_key) = &new_key {
            applied.plans += rename_plan(path, &old_key, new_key)?;
        }
    }

    applied.recorded = record::append(store, &history)?;
    verify_prefix(store, &prefix)?;
    verify_replay(store, &mapping)?;
    Ok(applied)
}

/// Refuses to finish with a key still off-prefix.
///
/// Re-lists the store rather than reusing the paths it started with: those files have been renamed, so checking the old
/// paths would read nothing and pass vacuously - a check that cannot fail is not a check.
fn verify_prefix(store: &Path, prefix: &str) -> Result<()> {
    let wanted = format!("{prefix}-");
    let remaining = store_tickets(store)
        .iter()
        .filter_map(|path| read_refs(path))
        .filter(|refs| !refs.key.starts_with(&wanted))
        .count();
    if remaining > 0 {
        anyhow::bail!("{remaining} key(s) still off-prefix after the remint");
    }
    Ok(())
}
