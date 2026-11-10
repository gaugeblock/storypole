//! Applying a remint to one store (`GB-347` step 3).
//!
//! One store at a time, deliberately: applying across eleven repositories in one command would be a single irreversible
//! step with nowhere to bisect.
//!
//! Keys, edge targets, citations and plan filenames move in the same pass, because a key that moves without its
//! references leaves a live graph pointing at nothing.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use storypole_core::ticket_keys::{Remint, plan_remint};

use super::remint_cmds::{cites, prefix_of, read_refs};
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
}

/// Renames every off-prefix key in the store at `store`, rewriting what points at them.
///
/// # Errors
///
/// Returns an error when the store's prefix is unknown, when a ticket file cannot be read or written, or when any key
/// is still off-prefix afterwards.
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
    for path in &files {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("could not read {}", path.display()))?;
        let mut document: serde_json::Value = serde_json::from_str(&text)
            .with_context(|| format!("{} is not valid JSON", path.display()))?;

        let old_key = document
            .get("ticket")
            .and_then(|ticket| ticket.get("ticket_key"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let new_key = lookup.get(&old_key).cloned();

        if let Some(ticket) = document.get_mut("ticket") {
            if let Some(new_key) = &new_key {
                ticket["ticket_key"] = serde_json::Value::String(new_key.clone());
                applied.keys += 1;
            }
            let (edges, citations) = rewrite_task(ticket, &lookup, &mapping);
            applied.edges += edges;
            applied.citations += citations;
        }

        // The event log carries a full snapshot per event, and the index is built by replaying it. Rewriting only the
        // projection left every replay resurrecting the old key: the ticket said BA-1 and `storypole list` still showed
        // GB-112, from the events. The log is live state, not history - it is what the next rebuild reads (GB-347).
        if let Some(events) = document
            .get_mut("events")
            .and_then(|events| events.as_array_mut())
        {
            for snapshot in events.iter_mut() {
                if let Some(task) = snapshot.get_mut("task") {
                    if let Some(new_key) = &new_key
                        && task.get("ticket_key").and_then(serde_json::Value::as_str)
                            == Some(old_key.as_str())
                    {
                        task["ticket_key"] = serde_json::Value::String(new_key.clone());
                    }
                    let (edges, citations) = rewrite_task(task, &lookup, &mapping);
                    applied.edges += edges;
                    applied.citations += citations;
                }
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

    verify(store, &prefix)?;
    Ok(applied)
}

/// Points every edge at the reminted key, when the edge names one that moved.
fn rewrite_edge_targets(
    ticket: &mut serde_json::Value,
    lookup: &BTreeMap<String, String>,
) -> usize {
    let Some(edges) = ticket
        .get_mut("edges")
        .and_then(|edges| edges.as_array_mut())
    else {
        return 0;
    };
    let mut rewritten = 0;
    for edge in edges.iter_mut() {
        let Some(target) = edge
            .get("target_ticket_key")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        if let Some(new_key) = lookup.get(&target) {
            edge["target_ticket_key"] = serde_json::Value::String(new_key.clone());
            rewritten += 1;
        }
    }
    rewritten
}

/// Rewrites whole-token citations of a moved key in the ticket body.
fn rewrite_citations(ticket: &mut serde_json::Value, mapping: &[Remint]) -> usize {
    let Some(original) = ticket
        .get("description")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
    else {
        return 0;
    };
    let mut body = original;
    let mut rewritten = 0;
    for remint in mapping {
        if cites(&body, &remint.from) {
            body = replace_token(&body, &remint.from, &remint.to);
            rewritten += 1;
        }
    }
    if rewritten > 0 {
        ticket["description"] = serde_json::Value::String(body);
    }
    rewritten
}

/// Replaces whole-token occurrences of `from` with `to`, leaving `GB-1` inside `GB-10` alone.
fn replace_token(text: &str, from: &str, to: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    while let Some(offset) = text[cursor..].find(from) {
        let at = cursor + offset;
        let before = text[..at].chars().next_back();
        let after = text[at + from.len()..].chars().next();
        let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric() && c != '-');
        out.push_str(&text[cursor..at]);
        out.push_str(if boundary(before) && boundary(after) {
            to
        } else {
            from
        });
        cursor = at + from.len();
    }
    out.push_str(&text[cursor..]);
    out
}

/// Refuses to finish with a key still off-prefix.
///
/// Re-lists the store rather than reusing the paths it started with: those files have been renamed, so checking the old
/// paths would read nothing and pass vacuously - a check that cannot fail is not a check.
fn verify(store: &Path, prefix: &str) -> Result<()> {
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

/// Applies every reference rewrite to one task-shaped object: edge targets and body citations.
///
/// Used for the projection and for each event snapshot, because they are the same object shape and both are read: the
/// projection by everything that opens a ticket file, the snapshots by every index rebuild.
fn rewrite_task(
    task: &mut serde_json::Value,
    lookup: &BTreeMap<String, String>,
    mapping: &[Remint],
) -> (usize, usize) {
    (
        rewrite_edge_targets(task, lookup),
        rewrite_citations(task, mapping),
    )
}
