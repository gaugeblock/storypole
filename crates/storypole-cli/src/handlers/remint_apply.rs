//! Applying a remint to one store (`GB-347` step 3).
//!
//! One store at a time, deliberately: applying across eleven repositories in one command would be a single irreversible
//! step with nowhere to bisect.
//!
//! Keys, edge targets, citations and plan filenames move in the same pass, because a key that moves without its
//! references leaves a live graph pointing at nothing - and a missing edge target renders as a graph with fewer links
//! rather than as a broken one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use storypole_core::ticket_keys::plan_remint;

use super::remint_cmds::{cites, prefix_of, read_refs};

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
/// Returns an error when the store's prefix is unknown, when a ticket file cannot be read or written, or when the
/// post-condition check finds a key still off-prefix afterwards.
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
            applied.edges += rewrite_edge_targets(ticket, &lookup);
            applied.citations += rewrite_citations(ticket, &mapping);
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
            applied.plans += rename_plan(store, &old_key, new_key)?;
        }
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
    fn rewrite_citations(
        ticket: &mut serde_json::Value,
        mapping: &[storypole_core::ticket_keys::Remint],
    ) -> usize {
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

    /// Renames the plan blueprint named after a ticket, returning 1 when one moved.
    fn rename_plan(store: &Path, old_key: &str, new_key: &str) -> Result<usize> {
        let plans = store.join("plans");
        let from = plans.join(format!("{old_key}.md"));
        if !from.exists() {
            return Ok(0);
        }
        let to = plans.join(format!("{new_key}.md"));
        std::fs::rename(&from, &to)
            .with_context(|| format!("could not rename {} to {}", from.display(), to.display()))?;
        Ok(1)
    }

    /// Refuses to finish with a key still off-prefix.
    fn verify(store: &Path, prefix: &str) -> Result<()> {
        let wanted = format!("{prefix}-");
        let remaining = store_tickets(store)
            .iter()
            .filter_map(|path| read_refs(path))
            .filter(|refs| !refs.key.starts_with(&wanted))
            .count();
        if remaining > 0 {
            anyhow::bail!(
                "{remaining} key(s) still off-prefix in {} after the remint",
                store.display()
            );
        }
        Ok(())
    }

    verify(store, &prefix)?;
    Ok(applied)
}

/// The ticket files of the store at `store` itself - not its members, because applying is one store at a time.
pub(crate) fn store_tickets(store: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for base in [
        store.join(".gaugeblock").join("storypole"),
        store.join(".storypole"),
    ] {
        let Ok(entries) = std::fs::read_dir(base.join("tickets")) else {
            continue;
        };
        paths.extend(
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "json")),
        );
    }
    paths
}
