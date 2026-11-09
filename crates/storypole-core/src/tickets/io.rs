//! Reading and writing one ticket record (`GB-329`).
//!
//! A record is rewritten whole, which is why every write is a rename of a completed temporary file and
//! every read-modify-write takes the per-ticket lock.

use std::fs::{self, OpenOptions};
use std::io::Write;
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

/// Publishes a new record, refusing to replace an existing one (`GB-345`).
///
/// # Portability
///
/// A hard link would decide the winner in one syscall, and it is what this used at first - but hard links do
/// not exist on exFAT or FAT32, which is what an external drive is usually formatted with, and they are
/// refused by some network filesystems. A checkout there would fail to create a ticket at all, so exclusion
/// is built from `O_CREAT | O_EXCL` instead: universal, and specified to be atomic over NFS as well.
///
/// The sequence is: reserve the key with an exclusive claim file, write the record completely, rename it into
/// place, drop the claim. The rename is what makes the record appear whole, so reserving the name and
/// publishing the contents stay separate concerns.
///
/// Returns `false` when the key is already claimed - a collision for the caller to retry with the next
/// number, not an error, because nothing is wrong with either request.
///
/// # Errors
///
/// [`StorypoleError::TicketRecord`] when the record cannot be written or published.
pub fn create(path: &Path, record: &TicketRecord) -> Result<bool, StorypoleError> {
    create_with_stale_after(path, record, super::lock::STALE_MS)
}

/// [`create`], with the age at which an abandoned claim may be taken over.
///
/// The threshold is a parameter so a test can force a takeover rather than sleeping for thirty seconds.
pub(crate) fn create_with_stale_after(
    path: &Path,
    record: &TicketRecord,
    stale_after_ms: u64,
) -> Result<bool, StorypoleError> {
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
    let claim = parent.join(format!(".{stem}.claim"));

    // Two attempts: the second only happens after taking over an abandoned claim.
    for attempt in 0..2 {
        match OpenOptions::new().write(true).create_new(true).open(&claim) {
            Ok(mut held) => {
                let _ = writeln!(held, "{}", std::process::id());
                // Holding the claim means no other creator is mid-flight for this key, so a record that
                // already exists was completed by an earlier creation and the key is taken.
                if path.exists() {
                    let _ = fs::remove_file(&claim);
                    return Ok(false);
                }
                let published = write(path, record);
                let _ = fs::remove_file(&claim);
                return published.map(|()| true);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if attempt == 0 && super::lock::is_stale_after(&claim, stale_after_ms) {
                    // The previous writer died holding the claim; take it over.
                    let _ = fs::remove_file(&claim);
                    continue;
                }
                return Ok(false);
            }
            Err(e) => {
                return Err(StorypoleError::TicketRecord {
                    path: claim,
                    reason: e.to_string(),
                });
            }
        }
    }
    Ok(false)
}
