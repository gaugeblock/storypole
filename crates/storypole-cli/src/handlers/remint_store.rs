//! Where a store keeps its tickets and plans (`GB-347`).
//!
//! Split out when `remint_apply` crossed the 250-line limit, and worth separating on its own terms: these two answer
//! "where is it" rather than "what to do with it", and the plans path is derived here once because deriving it twice -
//! from the ticket in one place and from the store root in another - is the bug that made a dry-run count promise a
//! rename the apply never performed.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

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

/// Renames the plan blueprint named after a ticket, returning 1 when one moved.
///
/// The plans directory is derived from the ticket path, exactly as `read_refs` derives it when counting. Taking it from
/// the store root instead put it one directory too high, so the dry run promised a rename the apply never performed.
pub(crate) fn rename_plan(ticket_path: &Path, old_key: &str, new_key: &str) -> Result<usize> {
    let Some(plans) = ticket_path
        .parent()
        .and_then(|tickets| tickets.parent())
        .map(|store| store.join("plans"))
    else {
        return Ok(0);
    };
    let from = plans.join(format!("{old_key}.md"));
    if !from.exists() {
        return Ok(0);
    }
    let to = plans.join(format!("{new_key}.md"));
    std::fs::rename(&from, &to)
        .with_context(|| format!("could not rename {} to {}", from.display(), to.display()))?;
    Ok(1)
}
