//! Finding the files an audit reads (`GB-345`).
//!
//! Two sources, one shape: the pre-migration append-only ledgers, and the one-file-per-ticket stores that replaced them
//! (ADR 0010). Keeping both here means "where does a repository keep its events?" has one answer in one place, and the
//! audit asks it once instead of hard-coding paths in two loops.

use std::path::{Path, PathBuf};

/// Finds the root ledger and every member ledger under `root`.
///
/// Pre-migration source: the append-only `ledger.jsonl` files. A store that no longer keeps one returns an empty
/// list, which is why [`discover_ticket_files`] exists beside it (`GB-345`).
#[must_use]
pub fn discover_ledgers(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    let root_ledger = gaugeblock_workspace::GaugeblockPaths::at(root).ledger();
    if root_ledger.exists() {
        paths.push(root_ledger);
    }

    if let Ok(entries) = std::fs::read_dir(root) {
        let mut members: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .map(|dir| dir.join(".storypole").join("ledger.jsonl"))
            .filter(|path| path.exists())
            .collect();
        members.sort();
        paths.extend(members);
    }

    paths
}

pub fn discover_ticket_files(root: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![root.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut members: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        members.sort();
        dirs.extend(members);
    }

    let mut paths = Vec::new();
    for dir in dirs {
        for base in [
            dir.join(".gaugeblock").join("storypole"),
            dir.join(".storypole"),
        ] {
            let Ok(entries) = std::fs::read_dir(base.join("tickets")) else {
                continue;
            };
            let mut found: Vec<PathBuf> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
                .collect();
            found.sort();
            paths.extend(found);
        }
    }
    paths
}
