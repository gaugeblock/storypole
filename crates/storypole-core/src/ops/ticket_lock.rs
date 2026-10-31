//! Cross-process exclusion for ticket-key allocation (`GB-185`).
//!
//! A ticket key is `max(existing) + 1`, read from the index and written back by the ledger event. Two
//! concurrent `defer` calls therefore read the same maximum and both claim the next key: the ledger ends
//! up with two tasks called `GB-237`, and every later lookup by key - `claim`, `verify`, `show` -
//! resolves one of them arbitrarily. Nothing reports an error, which is what "poison the index" means.
//!
//! `create_new` is atomic on every platform this runs on, so an exclusively created lock file is enough.
//! It is held only across allocation-and-append, so the wait is short and a stale file from a crashed
//! process is recovered by age rather than left to block the suite forever.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::error::StorypoleError;

/// How long to wait for another process to finish allocating.
const WAIT: Duration = Duration::from_secs(5);

/// How often to retry while waiting.
const POLL: Duration = Duration::from_millis(25);

/// Age past which an existing lock is assumed abandoned.
const STALE: Duration = Duration::from_secs(10);

/// The lock file name, inside the store's own directory.
const LOCK_NAME: &str = ".ticket-alloc.lock";

/// An exclusive claim on ticket-key allocation.
#[derive(Debug)]
pub struct TicketLock {
    path: PathBuf,
}

impl TicketLock {
    /// Waits for exclusive access to key allocation in `dot_dir`.
    ///
    /// # Errors
    ///
    /// [`StorypoleError::IoWithPath`] when the lock cannot be created for a reason other than
    /// contention, and a timeout error when another process holds it for longer than [`WAIT`].
    pub fn acquire(dot_dir: &Path) -> Result<Self, StorypoleError> {
        let path = dot_dir.join(LOCK_NAME);
        let deadline = SystemTime::now() + WAIT;
        loop {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => {
                    // Write something identifying but small; the content is for a human reading the
                    // directory, not for the protocol.
                    drop(file);
                    return Ok(Self { path });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if Self::is_stale(&path) {
                        // A crashed run left it behind; removing it is safe because only a live
                        // allocator would have refreshed the mtime.
                        let _ = std::fs::remove_file(&path);
                        continue;
                    }
                    if SystemTime::now() >= deadline {
                        return Err(StorypoleError::IoWithPath {
                            path,
                            source: std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                "another process is allocating a ticket key",
                            ),
                        });
                    }
                    std::thread::sleep(POLL);
                }
                Err(source) => return Err(StorypoleError::IoWithPath { path, source }),
            }
        }
    }

    fn is_stale(path: &Path) -> bool {
        std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age > STALE)
    }
}

impl Drop for TicketLock {
    fn drop(&mut self) {
        // Best effort: a leftover file is recovered by `is_stale` on the next run.
        let _ = std::fs::remove_file(&self.path);
    }
}
