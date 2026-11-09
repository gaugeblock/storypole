//! Lease files on disk, and the lock that serialises them (`GB-333`).
//!
//! Split from the lease rules so both stay readable: this module knows how a lease is stored, and the
//! parent knows what a lease means.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use super::Lease;
use crate::error::StorypoleError;
use crate::tickets::lock::is_stale_after;

/// How long to wait for the lease lock before giving up.
const WAIT_MS: u64 = 5_000;

/// Poll interval while waiting.
const POLL: Duration = Duration::from_millis(10);
pub(crate) fn lease_path(dir: &Path, key: &str) -> PathBuf {
    dir.join(format!("{key}.json"))
}

pub(crate) fn read_lease(dir: &Path, key: &str) -> Result<Option<Lease>, StorypoleError> {
    let path = lease_path(dir, key);
    let Ok(bytes) = fs::read(&path) else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| StorypoleError::TicketRecord {
            path,
            reason: e.to_string(),
        })
}

pub(crate) fn write_lease(dir: &Path, lease: &Lease) -> Result<(), StorypoleError> {
    fs::create_dir_all(dir).map_err(|e| StorypoleError::TicketRecord {
        path: dir.to_path_buf(),
        reason: e.to_string(),
    })?;
    let path = lease_path(dir, &lease.ticket);
    let temp = dir.join(format!(".{}.{}.tmp", lease.ticket, std::process::id()));
    let bytes = serde_json::to_vec_pretty(lease).map_err(|e| StorypoleError::TicketRecord {
        path: path.clone(),
        reason: e.to_string(),
    })?;
    fs::write(&temp, &bytes).map_err(|e| StorypoleError::TicketRecord {
        path: temp.clone(),
        reason: e.to_string(),
    })?;
    fs::rename(&temp, &path).map_err(|e| {
        let _ = fs::remove_file(&temp);
        StorypoleError::TicketRecord {
            path: path.clone(),
            reason: e.to_string(),
        }
    })
}

pub(crate) fn remove(dir: &Path, key: &str) -> Result<(), StorypoleError> {
    let path = lease_path(dir, key);
    fs::remove_file(&path).map_err(|e| StorypoleError::TicketRecord {
        path,
        reason: e.to_string(),
    })
}

/// Exclusive access to one lease file, released on drop.
#[derive(Debug)]
pub(crate) struct LeaseLock {
    path: PathBuf,
}

impl LeaseLock {
    pub(crate) fn acquire(dir: &Path, key: &str) -> Result<Self, StorypoleError> {
        fs::create_dir_all(dir).map_err(|e| StorypoleError::TicketRecord {
            path: dir.to_path_buf(),
            reason: e.to_string(),
        })?;
        let path = dir.join(format!(".{key}.lock"));
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
            reason: format!("lease lock still held after {WAIT_MS}ms"),
        })
    }
}

impl Drop for LeaseLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
