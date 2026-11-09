//! Per-ticket advisory lock (`GB-329`).
//!
//! A ticket record is rewritten whole, so two writers race: the second reads, the first writes, the
//! second writes and the first event is gone. The ledger avoided this with one `O_APPEND` write per
//! event, which a whole-file record cannot do - so the lock is the replacement for that guarantee.
//!
//! It is an **advisory** lock built from `O_CREAT | O_EXCL`, because storypole denies `unsafe_code` and
//! has no file-locking dependency. A stale lock (a killed process) is taken over after [`STALE_MS`],
//! which is longer than any single record write should take and short enough that a crash does not wedge
//! a ticket for a working session.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::thread;
use std::time::{Duration, SystemTime};

use crate::error::StorypoleError;

/// Age after which an existing lock file is assumed to belong to a dead process.
pub const STALE_MS: u64 = 30_000;

/// Total time to wait for a lock before giving up.
const WAIT_MS: u64 = 5_000;

/// Poll interval while waiting.
const POLL: Duration = Duration::from_millis(10);

/// Held for as long as a record is being read-modify-written; releases on drop.
#[derive(Debug)]
pub struct TicketLock {
    path: std::path::PathBuf,
}

impl TicketLock {
    /// Takes the lock for `key`, waiting briefly and taking over a stale one.
    ///
    /// # Errors
    ///
    /// [`StorypoleError::TicketRecord`] when the lock stays held for longer than [`WAIT_MS`].
    pub fn acquire(dot_dir: &Path, key: &str) -> Result<Self, StorypoleError> {
        let dir = super::dir(dot_dir);
        fs::create_dir_all(&dir).map_err(|e| StorypoleError::TicketRecord {
            path: dir.clone(),
            reason: e.to_string(),
        })?;
        let path = dir.join(format!("{key}.lock"));
        let mut waited = 0u64;

        while waited < WAIT_MS {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    // Best effort: the pid is a diagnostic, not part of the protocol.
                    let _ = writeln!(file, "{}", std::process::id());
                    return Ok(Self { path });
                }
                Err(_) if is_stale(&path) => {
                    // A process that died mid-write leaves the file behind; take it over.
                    let _ = fs::remove_file(&path);
                }
                Err(_) => {
                    thread::sleep(POLL);
                    waited += POLL.as_millis() as u64;
                }
            }
        }

        Err(StorypoleError::TicketRecord {
            path,
            reason: format!("lock still held after {WAIT_MS}ms"),
        })
    }
}

impl Drop for TicketLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Whether the lock file is old enough to be a dead process's.
fn is_stale(path: &Path) -> bool {
    is_stale_after(path, STALE_MS)
}

/// Whether `path` has not been touched for longer than `stale_after_ms`.
///
/// Shared with record publication, which uses the same reasoning for its claim file and takes the threshold
/// as a parameter so a test can force the takeover instead of sleeping for thirty seconds.
pub(crate) fn is_stale_after(path: &Path, stale_after_ms: u64) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    let Ok(age) = SystemTime::now().duration_since(modified) else {
        return false;
    };
    // Greater-or-equal rather than greater: as_millis truncates, so a file created microseconds ago
    // reports an age of zero and a zero threshold would never fire. At thirty seconds it makes no
    // difference.
    age.as_millis() as u64 >= stale_after_ms
}
