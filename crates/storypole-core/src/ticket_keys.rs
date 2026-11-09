//! Ticket numbers from one counter per repository (`GB-347`).
//!
//! A number used to be "highest in the local index plus one". The index describes one working tree, so two
//! branches of one repository each counted from their own highest and both minted `SP-1` for different
//! tickets. Nothing failed at the time: the collision surfaced at merge, as an add/add conflict on one file,
//! with no specified resolution - keep one side and lose the other agent's ticket.
//!
//! Numbers therefore come from a counter that lives in the **git directory**, which is the one place that is
//! shared by every checkout of a repository and is never tracked. No ignore rules, no migration of tracked
//! files, and no way for a worktree to have its own copy: `ADR 0009` says derived and coordination state
//! follows the checkout, and this is coordination state.
//!
//! The allocation takes `max(counter, index)` so a repository whose history predates the counter continues
//! from the numbers already in use instead of re-issuing them.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::error::StorypoleError;
use crate::tickets::lock::is_stale_after;

/// How long to wait for the allocation lock before giving up.
const WAIT_MS: u64 = 5_000;

/// Poll interval while waiting.
const POLL: Duration = Duration::from_millis(10);

/// The shared counter directory for the repository containing `root_dir`.
///
/// `None` when `root_dir` is not in a checkout, which is legal - a scratch directory has one working tree and
/// no repository, so the caller falls back to the store directory.
#[must_use]
pub fn shared_dir(root_dir: &Path) -> Option<PathBuf> {
    let repository = gaugeblock_git::Repository::discover(root_dir).ok()?;
    let common_dir = repository.common_dir().ok()?;
    Some(common_dir.join("storypole").join("keys"))
}

/// Takes the next number for `prefix`, never returning one at or below `index_max`.
///
/// # Errors
///
/// [`StorypoleError::TicketRecord`] when the lock cannot be taken or the counter cannot be read or written.
pub fn allocate(dir: &Path, index_max: u64, prefix: &str) -> Result<u64, StorypoleError> {
    fs::create_dir_all(dir).map_err(|e| StorypoleError::TicketRecord {
        path: dir.to_path_buf(),
        reason: e.to_string(),
    })?;
    let name = prefix.to_lowercase();
    let _lock = AllocateLock::acquire(dir, &name)?;

    let counter = dir.join(format!("{name}.seq"));
    let stored = fs::read_to_string(&counter)
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let next = stored.max(index_max) + 1;
    fs::write(&counter, next.to_string()).map_err(|e| StorypoleError::TicketRecord {
        path: counter,
        reason: e.to_string(),
    })?;
    Ok(next)
}

/// Exclusive access to one prefix's counter, released on drop.
#[derive(Debug)]
struct AllocateLock {
    path: PathBuf,
}

impl AllocateLock {
    fn acquire(dir: &Path, name: &str) -> Result<Self, StorypoleError> {
        let path = dir.join(format!(".{name}.lock"));
        let mut waited = 0u64;
        while waited < WAIT_MS {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut held) => {
                    let _ = writeln!(held, "{}", std::process::id());
                    return Ok(Self { path });
                }
                Err(_) if is_stale_after(&path, crate::tickets::lock::STALE_MS) => {
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
            reason: format!("allocation counter still locked after {WAIT_MS}ms"),
        })
    }
}

impl Drop for AllocateLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn numbers_come_from_the_counter_rather_than_the_index() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("keys");

        assert_eq!(allocate(&dir, 0, "SP").unwrap(), 1);
        assert_eq!(allocate(&dir, 0, "SP").unwrap(), 2);
    }

    #[test]
    fn a_repository_with_existing_tickets_continues_from_its_index() {
        // The migration case: records exist but the counter does not yet, so re-issuing from zero would
        // duplicate every key already in use.
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("keys");

        assert_eq!(allocate(&dir, 41, "SP").unwrap(), 42);
        assert_eq!(allocate(&dir, 41, "SP").unwrap(), 43);
    }

    #[test]
    fn prefixes_do_not_share_a_counter() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("keys");

        assert_eq!(allocate(&dir, 0, "SP").unwrap(), 1);
        assert_eq!(
            allocate(&dir, 0, "MT").unwrap(),
            1,
            "each prefix counts itself"
        );
    }

    #[test]
    fn the_counter_is_not_left_under_a_lock() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("keys");
        allocate(&dir, 0, "SP").unwrap();

        let leftovers: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".lock"))
            .collect();
        assert!(leftovers.is_empty(), "locks do not linger: {leftovers:?}");
    }
}
