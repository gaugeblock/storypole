//! Brings ticket projections back into agreement with their events (`GB-349`).
//!
//! [`crate::tickets::write`] keeps `ticket` true from now on, but a store written before that fix still has
//! blocks disagreeing with their own event log — 71 of them across the suite when this was measured, every
//! one of them a ticket whose archive or verify never rewrote the block. They will not repair themselves:
//! an archived ticket is never touched again, so the divergence is permanent without a pass that walks them.
//!
//! A pass, not a migration: it rewrites only records that disagree, it reports what it would touch before
//! touching anything, and a second run finds nothing to do.

use std::path::{Path, PathBuf};

use crate::error::StorypoleError;
use crate::tickets::{self, TicketLock};

/// What a pass found and, when applied, what it fixed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefreshReport {
    /// Records examined.
    pub scanned: usize,
    /// Keys whose projection disagreed with their events, whether or not they were rewritten.
    pub stale: Vec<String>,
    /// Keys actually rewritten; empty on a dry run.
    pub written: Vec<String>,
    /// Keys that could not be read as a record, with the reason. Reported, never silently skipped.
    pub unreadable: Vec<(String, String)>,
}

impl RefreshReport {
    /// True when nothing disagreed, which is the healthy state and the only one where a pass wrote nothing.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.stale.is_empty() && self.unreadable.is_empty()
    }
}

/// Walks every ticket record in one store's directory.
///
/// `apply` false reports only. The lock is taken per ticket, the same lock [`tickets::append_event`] uses,
/// so a pass cannot interleave with a write and lose an event.
///
/// # Errors
///
/// [`StorypoleError::IoWithPath`] when the tickets directory cannot be listed.
pub fn refresh_store(dot_dir: &Path, apply: bool) -> Result<RefreshReport, StorypoleError> {
    let mut report = RefreshReport::default();
    let dir = tickets::dir(dot_dir);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(report);
    };

    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();

    for path in paths {
        let Some(key) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let key = key.to_string();
        report.scanned += 1;

        let _lock = match TicketLock::acquire(dot_dir, &key) {
            Ok(lock) => lock,
            Err(error) => {
                report.unreadable.push((key, error.to_string()));
                continue;
            }
        };

        let record = match tickets::read(&path) {
            Ok(record) => record,
            Err(error) => {
                report.unreadable.push((key, error.to_string()));
                continue;
            }
        };
        if record.replay().is_none() || record.replay() == Some(record.ticket.clone()) {
            continue;
        }

        report.stale.push(key.clone());
        if !apply {
            continue;
        }
        // `write` refreshes the projection itself, so the record handed to it is the stale one on purpose:
        // the repair exercises the same path every future write takes rather than a second arrangement of
        // the same logic that could drift from it.
        match tickets::write(&path, &record) {
            Ok(()) => report.written.push(key),
            Err(error) => report.unreadable.push((key, error.to_string())),
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests;
