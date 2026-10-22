//! Append-only JSONL ledger reader and writer with git conflict resilience.

pub mod replay;

use crate::error::StorypoleError;
use crate::model::LedgerEvent;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

/// Append a single `LedgerEvent` to the JSONL file at `ledger_path`.
pub fn append_event(ledger_path: &Path, event: &LedgerEvent) -> Result<(), StorypoleError> {
    if let Some(parent) = ledger_path.parent() {
        fs::create_dir_all(parent).map_err(|e| StorypoleError::IoWithPath {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(ledger_path)
        .map_err(|e| StorypoleError::IoWithPath {
            path: ledger_path.to_path_buf(),
            source: e,
        })?;

    let line = serde_json::to_string(event)?;
    writeln!(file, "{line}").map_err(|e| StorypoleError::IoWithPath {
        path: ledger_path.to_path_buf(),
        source: e,
    })?;
    file.flush().map_err(|e| StorypoleError::IoWithPath {
        path: ledger_path.to_path_buf(),
        source: e,
    })?;
    Ok(())
}

/// Read all `LedgerEvent`s from `ledger_path`, gracefully skipping blank lines and git merge markers.
pub fn read_events(ledger_path: &Path) -> Result<Vec<LedgerEvent>, StorypoleError> {
    if !ledger_path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(ledger_path).map_err(|e| StorypoleError::IoWithPath {
        path: ledger_path.to_path_buf(),
        source: e,
    })?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for (idx, line_res) in reader.lines().enumerate() {
        let line_num = idx + 1;
        let line = line_res.map_err(|e| StorypoleError::IoWithPath {
            path: ledger_path.to_path_buf(),
            source: e,
        })?;
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        // Ignore git merge conflict markers gracefully
        if trimmed.starts_with("<<<<<<<")
            || trimmed.starts_with("=======")
            || trimmed.starts_with(">>>>>>>")
            || trimmed.starts_with("|||||||")
        {
            tracing::warn!(
                line = line_num,
                marker = trimmed,
                "Skipping git merge conflict marker in ledger"
            );
            continue;
        }

        let event: LedgerEvent =
            serde_json::from_str(trimmed).map_err(|e| StorypoleError::CorruptLedger {
                line: line_num,
                reason: e.to_string(),
            })?;
        events.push(event);
    }

    Ok(events)
}
