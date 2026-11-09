//! Claim leases in the shared layer (`GB-333`, ADR 0009).
//!
//! A lease decides who may work on a ticket, so every checkout of a repository must see it before any branch
//! is merged. The ticket record cannot carry it: records follow the branch, so a claim made in one worktree
//! would be invisible in another and two agents would take the same ticket - the collision the lease exists
//! to prevent. Leases therefore live beside the ticket-number counter, in the git directory: shared per
//! repository by construction and never tracked. The ticket file still records that a claim *happened*,
//! because that is history and belongs to the branch.

pub mod store;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::StorypoleError;

/// One agent's hold on one ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lease {
    /// Ticket the lease covers.
    pub ticket: String,
    /// Agent holding it.
    pub agent: String,
    /// When the hold lapses, epoch milliseconds.
    pub expires_at_ms: u64,
}

/// The outcome of asking for a lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Acquire {
    /// The caller now holds it.
    Granted,
    /// Someone else holds it and has not expired.
    Held(Lease),
}

/// The shared lease directory for the repository containing `root_dir`.
///
/// `None` outside a checkout, where the caller falls back to the store directory: one working tree means one
/// copy of the state and nothing to coordinate.
#[must_use]
pub fn dir_for(root_dir: &Path) -> Option<PathBuf> {
    let repository = gaugeblock_git::Repository::discover(root_dir).ok()?;
    let common_dir = repository.common_dir().ok()?;
    Some(common_dir.join("storypole").join("leases"))
}

/// Takes the lease on `key` for `agent`, or reports the holder.
///
/// An expired lease is taken over rather than refused: a lease exists so a dead agent stops blocking work,
/// and the alternative is a ticket nobody can claim until someone cleans up by hand.
///
/// # Errors
///
/// [`StorypoleError::TicketRecord`] when the lease file cannot be read or written.
pub fn acquire(
    dir: &Path,
    key: &str,
    agent: &str,
    expires_at_ms: u64,
    now_ms: u64,
) -> Result<Acquire, StorypoleError> {
    let _lock = store::LeaseLock::acquire(dir, key)?;
    match store::read_lease(dir, key)? {
        Some(lease) if lease.expires_at_ms > now_ms && lease.agent != agent => {
            Ok(Acquire::Held(lease))
        }
        _ => {
            store::write_lease(
                dir,
                &Lease {
                    ticket: key.to_string(),
                    agent: agent.to_string(),
                    expires_at_ms,
                },
            )?;
            Ok(Acquire::Granted)
        }
    }
}

/// Drops the lease on `key` when `agent` holds it, reporting whether anything was released.
///
/// A refusal is not an error: an agent that never held the lease has nothing to release.
///
/// # Errors
///
/// [`StorypoleError::TicketRecord`] when the lease file cannot be read or removed.
pub fn release(dir: &Path, key: &str, agent: &str) -> Result<bool, StorypoleError> {
    let _lock = store::LeaseLock::acquire(dir, key)?;
    match store::read_lease(dir, key)? {
        Some(lease) if lease.agent == agent => {
            store::remove(dir, key)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Drops the lease on `key` regardless of who holds it.
///
/// For a handoff, which is a deliberate release, and for reaping, where the holder is by definition gone.
///
/// # Errors
///
/// [`StorypoleError::TicketRecord`] when the lease file cannot be read or removed.
pub fn force_release(dir: &Path, key: &str) -> Result<bool, StorypoleError> {
    let _lock = store::LeaseLock::acquire(dir, key)?;
    if store::read_lease(dir, key)?.is_some() {
        store::remove(dir, key)?;
        return Ok(true);
    }
    Ok(false)
}

/// Takes the shared lease for a ticket on behalf of a store, mapping a conflict onto the error a caller
/// already reports.
///
/// The store decides where the lease directory is and how long a claim lasts, so a caller does not repeat
/// that reasoning. `GB-333`.
///
/// # Errors
///
/// [`StorypoleError::ClaimConflict`] when another checkout holds a live lease, [`StorypoleError::TicketRecord`]
/// when the lease cannot be read or written.
pub fn claim_for(
    store: &crate::store::StorypoleStore,
    key: &str,
    agent: &str,
    task_id: uuid::Uuid,
) -> Result<(), StorypoleError> {
    let dir = dir_for_store(store.root_dir(), store.dot_dir());
    let now = crate::store::StorypoleStore::now_ms();
    let ttl_ms = u64::from(store.config().claim_ttl_minutes).saturating_mul(60_000);
    match acquire(&dir, key, agent, now.saturating_add(ttl_ms), now)? {
        Acquire::Granted => Ok(()),
        Acquire::Held(lease) => Err(StorypoleError::ClaimConflict {
            task_id,
            current_agent: format!("{} (in another checkout)", lease.agent),
            expires_at: lease.expires_at_ms,
        }),
    }
}

/// The lease directory for a store: the repository's, or the store's own outside a checkout.
#[must_use]
pub fn dir_for_store(root_dir: &Path, dot_dir: &Path) -> PathBuf {
    dir_for(root_dir).unwrap_or_else(|| dot_dir.join("leases"))
}

/// The live holder of `key`, if any.
///
/// # Errors
///
/// [`StorypoleError::TicketRecord`] when the lease file cannot be read.
pub fn holder(dir: &Path, key: &str, now_ms: u64) -> Result<Option<Lease>, StorypoleError> {
    Ok(store::read_lease(dir, key)?.filter(|lease| lease.expires_at_ms > now_ms))
}

#[cfg(test)]
mod tests;
