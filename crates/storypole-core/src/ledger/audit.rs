//! Ledger integrity audit: does every line parse, and what does the real replay say?
//!
//! A distributed ledger set has no compiler. Eleven files appended by different stores can drift
//! from the index that reads them without anyone noticing: `GB-177` found the index reporting 116
//! Verified while the ledgers recorded 58 archived, with no way to ask which was wrong. This module
//! answers the structural half - parse failures, duplicates, and the replayed status histogram.
//! Reconciliation against the index is separate, because it needs the unified store.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::StorypoleError;
use crate::ledger::replay::replay_events;

/// Structural audit of one ledger file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedgerFileAudit {
    /// Path relative to the audited root.
    pub path: String,
    /// Lines that parsed into a ledger event.
    pub events: usize,
    /// Lines that did not parse.
    pub unparsable: usize,
    /// Lines identical to an earlier line in the same file.
    pub duplicates: usize,
}

/// Structural audit of the whole ledger set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedgerAudit {
    /// Per-file results, root first then members by name.
    pub files: Vec<LedgerFileAudit>,
    /// Lines examined across every file.
    pub total_events: usize,
    /// Lines that did not parse.
    pub unparsable: usize,
    /// Duplicate lines across the whole set.
    pub duplicates: usize,
    /// Tasks the replay produced.
    pub tasks: usize,
    /// Status histogram from the real replay, lowercase, excluding archived tasks.
    pub status_counts: BTreeMap<String, usize>,
    /// Tasks carrying an archived timestamp, counted separately because archived is a state.
    pub archived: usize,
}

impl LedgerAudit {
    /// `true` when every line parsed and nothing was duplicated.
    #[must_use]
    pub fn is_structurally_sound(&self) -> bool {
        self.unparsable == 0 && self.duplicates == 0
    }
}

/// Finds the root ledger and every member ledger under `root`.
#[must_use]
pub fn discover_ledgers(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    let root_ledger = root
        .join(".gaugeblock")
        .join("storypole")
        .join("ledger.jsonl");
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

/// Audits every ledger under `root` and replays what parsed.
///
/// # Errors
///
/// [`StorypoleError::Io`] when a ledger exists but cannot be read.
pub fn audit_ledgers(root: &Path) -> Result<LedgerAudit, StorypoleError> {
    let mut files = Vec::new();
    let mut events = Vec::new();
    let (mut total_events, mut unparsable, mut duplicates) = (0usize, 0usize, 0usize);

    for path in discover_ledgers(root) {
        let text = std::fs::read_to_string(&path).map_err(StorypoleError::Io)?;
        let mut seen: HashSet<&str> = HashSet::new();
        let (mut file_events, mut file_bad, mut file_dup) = (0usize, 0usize, 0usize);

        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            total_events += 1;
            if !seen.insert(line) {
                file_dup += 1;
                duplicates += 1;
            }
            match serde_json::from_str(line) {
                Ok(event) => {
                    file_events += 1;
                    events.push(event);
                }
                Err(_) => {
                    file_bad += 1;
                    unparsable += 1;
                }
            }
        }

        files.push(LedgerFileAudit {
            path: path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string(),
            events: file_events,
            unparsable: file_bad,
            duplicates: file_dup,
        });
    }

    let tasks = replay_events(&events);
    let mut status_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut archived = 0usize;
    for task in tasks.values() {
        // Archived is a state, not a status (GB-179): an archived task keeps its status, so it is
        // counted on its own or the status buckets would double-count it.
        if task.archived_at_epoch_ms.is_some() {
            archived += 1;
            continue;
        }
        *status_counts
            .entry(format!("{:?}", task.status).to_lowercase())
            .or_insert(0) += 1;
    }

    Ok(LedgerAudit {
        files,
        total_events,
        unparsable,
        duplicates,
        tasks: tasks.len(),
        status_counts,
        archived,
    })
}

/// Renders the audit for a terminal.
#[must_use]
pub fn format_ledger_audit(audit: &LedgerAudit) -> String {
    let mut out = String::new();
    for file in &audit.files {
        out.push_str(&format!(
            "{:>5} events  unparsable={:<3} dup={:<3} {}\n",
            file.events, file.unparsable, file.duplicates, file.path
        ));
    }
    out.push_str(&format!(
        "\n{} events across {} files: {} unparsable, {} duplicate\n",
        audit.total_events,
        audit.files.len(),
        audit.unparsable,
        audit.duplicates
    ));
    let histogram: Vec<String> = audit
        .status_counts
        .iter()
        .map(|(status, count)| format!("{status}={count}"))
        .collect();
    out.push_str(&format!(
        "replay: {} tasks — {}\n",
        audit.tasks,
        histogram.join(" ")
    ));
    out
}
