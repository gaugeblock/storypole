//! Reading and writing one ticket record (`GB-329`).
//!
//! A record is rewritten whole, which is why every write is a rename of a completed temporary file and
//! every read-modify-write takes the per-ticket lock.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::StorypoleError;
use crate::model::LedgerEvent;

use super::{TicketLock, TicketRecord, order_events, path};

/// Distinguishes temporary files written by concurrent callers in one process.
static WRITE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Reads one record.
///
/// # Errors
///
/// [`StorypoleError::TicketRecord`] when the file is missing or malformed. A missing record is not an
/// empty one: answering "no such ticket" for a corrupt file hides the corruption.
pub fn read(path: &Path) -> Result<TicketRecord, StorypoleError> {
    let bytes = fs::read(path).map_err(|e| StorypoleError::TicketRecord {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;
    serde_json::from_slice(&bytes).map_err(|e| StorypoleError::TicketRecord {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })
}

/// Writes one record atomically: a temporary file in the same directory, then a rename.
///
/// A rename within one filesystem is atomic, so a reader sees the old record or the new one and never a
/// half-written one - the usual failure being a process killed mid-write.
///
/// # Errors
///
/// [`StorypoleError::TicketRecord`] when the record cannot be serialised, written or renamed.
pub fn write(path: &Path, record: &TicketRecord) -> Result<(), StorypoleError> {
    let parent = path.parent().ok_or_else(|| StorypoleError::TicketRecord {
        path: path.to_path_buf(),
        reason: "record path has no parent directory".to_string(),
    })?;
    fs::create_dir_all(parent).map_err(|e| StorypoleError::TicketRecord {
        path: parent.to_path_buf(),
        reason: e.to_string(),
    })?;

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("record");
    let seq = WRITE_SEQ.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(".{stem}.{}.{seq}.tmp", std::process::id()));

    let bytes = serde_json::to_vec_pretty(record).map_err(|e| StorypoleError::TicketRecord {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;
    if let Err(e) = fs::write(&temp, &bytes) {
        let _ = fs::remove_file(&temp);
        return Err(StorypoleError::TicketRecord {
            path: temp,
            reason: e.to_string(),
        });
    }
    if let Err(e) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(StorypoleError::TicketRecord {
            path: path.to_path_buf(),
            reason: e.to_string(),
        });
    }
    Ok(())
}

/// Appends one event to a ticket's record under an exclusive lock.
///
/// The lock is what makes a read-modify-write safe when the CLI and the daemon, or two agents in one
/// checkout, touch the same ticket: the whole file is rewritten, so without it the last writer wins and
/// silently discards the other's event.
///
/// # Errors
///
/// [`StorypoleError::TicketRecord`] when the lock cannot be taken or the record cannot be read or written.
pub fn append_event(
    dot_dir: &Path,
    key: &str,
    event: &LedgerEvent,
) -> Result<TicketRecord, StorypoleError> {
    let record_path = path(dot_dir, key)?;
    let _lock = TicketLock::acquire(dot_dir, key)?;
    // A ticket's first event creates its file; every later event must find one. Answering "created" for a
    // status change on a ticket that does not exist would invent a record from nothing.
    let mut record = match read(&record_path) {
        Ok(record) => record,
        Err(error) => match seed_record(key, event) {
            Some(record) => record,
            None if record_path.exists() => return Err(error),
            None => {
                return Err(StorypoleError::TicketRecord {
                    path: record_path,
                    reason: format!("no record for {key} and this event does not carry one"),
                });
            }
        },
    };
    record.events.push(event.clone());
    order_events(&mut record.events);
    write(&record_path, &record)?;
    Ok(record)
}

/// A record for a ticket whose first event is in hand.
///
/// Only the two variants that carry a whole task can seed one; a `StatusChanged` has no title, kind or
/// scope, so there is nothing to write.
fn seed_record(key: &str, event: &LedgerEvent) -> Option<TicketRecord> {
    match event {
        LedgerEvent::TaskCreated { task } | LedgerEvent::DeferredItemCaptured { task, .. }
            if task.ticket_key == key =>
        {
            Some(TicketRecord::new(task.clone(), Vec::new()))
        }
        _ => None,
    }
}
