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

use crate::json_out::print_json;

/// Prints the remint mapping for every store under `root_dir`, or refuses to apply it.
///
/// # Errors
///
/// Returns an error when `apply` is requested - applying is a later step - or when a store's configuration cannot be
/// read, because a store whose prefix is unknown cannot be planned safely.
pub fn handle_remint(root_dir: &Path, apply: bool, json: bool) -> Result<()> {
    if apply {
        anyhow::bail!(
            "applying a remint is not implemented: `map` and dry run are GB-347 steps 1-2, and step 3 rewrites the reference graph in the same pass as the keys"
        );
    }

    // Group by store first: a prefix belongs to a repository, so the mapping only means anything per store.
    let mut stores: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    for path in discover_ticket_files(root_dir) {
        let Some(key) = read_key(&path) else {
            continue;
        };
        stores
            .entry(store_of(root_dir, &path))
            .or_default()
            .push(key);
    }

    let mut planned = Vec::new();
    let mut total = 0usize;
    for (store, mut keys) in stores {
        keys.sort();
        let prefix = prefix_of(&store).with_context(|| {
            format!(
                "no ticket prefix configured for {} - refusing to guess one",
                store.display()
            )
        })?;
        let mapping = plan_remint(&prefix, &keys);
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
            "\n{total} key(s) would be reminted across {} store(s). Nothing has been changed - applying this is GB-347 step 3, which rewrites edges, plans and prose in the same pass.",
            planned.len()
        );
    }
    Ok(())
}

/// The `ticket_key` inside one ticket file, if it is readable and shaped as expected.
fn read_key(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let document: serde_json::Value = serde_json::from_str(&text).ok()?;
    document
        .get("ticket")?
        .get("ticket_key")?
        .as_str()
        .map(str::to_string)
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
fn prefix_of(store: &Path) -> Result<String> {
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
