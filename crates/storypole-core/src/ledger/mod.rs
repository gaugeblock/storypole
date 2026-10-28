//! Append-only JSONL ledger reader and writer with git conflict resilience.

pub mod replay;

use crate::error::StorypoleError;
use crate::model::LedgerEvent;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

/// Append a single `LedgerEvent` to the JSONL file at `ledger_path`.
///
/// The event is serialised into one buffer and written with a **single `write` syscall** into a
/// file opened `O_APPEND`. That is what makes concurrent writers safe: the kernel places each
/// append at the end of the file without interleaving, whereas a formatted `writeln!` can issue
/// several writes and splice two events onto one line (observed twice, GB-147).
///
/// Before writing, the tail is checked: a ledger that does not end with a newline is refused rather
/// than appended to, so a corrupt tail fails loudly instead of swallowing the next event.
///
/// Note: atomic append relies on the local filesystem's append semantics. A network filesystem that
/// does not honour `O_APPEND` would need an advisory lock as well.
pub fn append_event(ledger_path: &Path, event: &LedgerEvent) -> Result<(), StorypoleError> {
    if let Some(parent) = ledger_path.parent() {
        fs::create_dir_all(parent).map_err(|e| StorypoleError::IoWithPath {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }

    ensure_clean_tail(ledger_path)?;

    let mut line = serde_json::to_string(event)?;
    line.push('\n');

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(ledger_path)
        .map_err(|e| StorypoleError::IoWithPath {
            path: ledger_path.to_path_buf(),
            source: e,
        })?;

    // One syscall, one event. `write_all` may retry on a short write, which is still correct
    // because every retry appends the remaining bytes at the new end of the file.
    if let Err(e) = file.write_all(line.as_bytes()) {
        return Err(StorypoleError::IoWithPath {
            path: ledger_path.to_path_buf(),
            source: e,
        });
    }
    file.flush().map_err(|e| StorypoleError::IoWithPath {
        path: ledger_path.to_path_buf(),
        source: e,
    })?;
    Ok(())
}

/// Refuses to append to a ledger whose last byte is not a newline.
///
/// Returns `Ok(())` for a missing or empty file, which is a valid starting point.
fn ensure_clean_tail(ledger_path: &Path) -> Result<(), StorypoleError> {
    use std::io::{Read, Seek, SeekFrom};

    let Ok(mut file) = fs::File::open(ledger_path) else {
        return Ok(());
    };
    let len = file
        .metadata()
        .map_err(|e| StorypoleError::IoWithPath {
            path: ledger_path.to_path_buf(),
            source: e,
        })?
        .len();
    if len == 0 {
        return Ok(());
    }

    file.seek(SeekFrom::End(-1))
        .map_err(|e| StorypoleError::IoWithPath {
            path: ledger_path.to_path_buf(),
            source: e,
        })?;
    let mut last = [0u8; 1];
    if file.read_exact(&mut last).is_err() {
        return Ok(());
    }
    if last[0] == b'\n' {
        return Ok(());
    }

    Err(StorypoleError::CorruptLedger {
        line: count_lines(ledger_path),
        reason: format!(
            "file does not end with a newline ({len} bytes); refusing to append to a partial tail — repair the ledger or delete the derived index first"
        ),
    })
}

/// Number of newline-terminated lines, used to point at the offending tail line.
fn count_lines(ledger_path: &Path) -> usize {
    let Ok(text) = fs::read_to_string(ledger_path) else {
        return 0;
    };
    text.lines().count().max(1)
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
