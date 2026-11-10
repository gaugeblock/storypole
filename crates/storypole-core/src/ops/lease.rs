//! Returning abandoned work to the backlog (`GB-257`).
//!
//! A claim is a lease with a TTL, but nothing enforced the TTL. When an agent stopped heartbeating the
//! task stayed `InProgress` for ever, so the board showed work nobody was doing - two tickets sat that
//! way for five days. [`AgentClaim::is_expired`] already existed and `claim` already allowed a takeover;
//! what was missing was anything that moved the status back.
//!
//! Reaping is an explicit command rather than a side effect of reading. A status change is a ledger
//! event, and a read that writes is a trap: `list` would mutate the ledger simply because someone looked
//! at it, and a read-only mount would fail. `--dry-run` gives the same visibility without the write.

use std::path::Path;

use crate::error::StorypoleError;
use crate::index::query::get_all_tasks;
use crate::model::LedgerEvent;
use crate::model::task::TaskStatus;
use crate::store::StorypoleStore;

/// One task returned to the backlog because its lease expired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReapedClaim {
    /// Ticket key of the task that was reclaimed.
    pub ticket_key: String,
    /// Agent that held the expired lease.
    pub agent_id: String,
    /// When the lease expired, in epoch milliseconds.
    pub expired_at_epoch_ms: u64,
}

/// Whether this store holds the ticket's record.
///
/// A store's index can list more than its own records: at a suite root it is the unified view, which holds every member's
/// tickets too. Reaping is a write, so it must be limited to what this store can actually record - otherwise the first
/// member ticket aborts the run with an error and the root's own releases are already half-written (`GB-352`).
fn owns(store: &StorypoleStore, key: &str) -> bool {
    // Either layout: a store still on `ledger.jsonl` holds its tickets there, and asking only about record files made
    // reaping a no-op for the whole legacy layout (caught by `an_expired_claim_is_reported_and_reaped_back_to_ready`).
    crate::workspace::lookup::holds(store.dot_dir(), key)
}

/// How to name the repository that owns a ticket, for a message a person can act on.
fn owner_label(root: &Path, dot_dir: &Path) -> String {
    match crate::workspace::lookup::repo_root_of(dot_dir)
        .strip_prefix(root)
        .ok()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
    {
        Some(name) => name.to_string(),
        None => "the workspace root".to_string(),
    }
}

/// Expired claims this store cannot reap, each with the repository to run `reap --dir` in.
///
/// # Errors
///
/// [`StorypoleError`] when the index cannot be read.
pub fn expired_claims_elsewhere(
    store: &StorypoleStore,
) -> Result<Vec<(String, String)>, StorypoleError> {
    let now = StorypoleStore::now_ms();
    let mut elsewhere = Vec::new();
    for task in get_all_tasks(store.conn())?.values() {
        if task.status != TaskStatus::InProgress || owns(store, &task.ticket_key) {
            continue;
        }
        let Some(claim) = task.active_claim.as_ref() else {
            continue;
        };
        if !claim.is_expired(now) {
            continue;
        }
        let owner = crate::workspace::lookup::locate_ticket(store.root_dir(), &task.ticket_key)
            .map_or_else(
                || "an unknown repository".to_string(),
                |dot| owner_label(store.root_dir(), &dot),
            );
        elsewhere.push((task.ticket_key.clone(), owner));
    }
    elsewhere.sort();
    Ok(elsewhere)
}

/// Tasks that are `InProgress` while their claim lease has expired, without changing anything.
///
/// # Errors
///
/// [`StorypoleError`] when the index cannot be read.
pub fn expired_claims(store: &StorypoleStore) -> Result<Vec<ReapedClaim>, StorypoleError> {
    let now = StorypoleStore::now_ms();
    let tasks = get_all_tasks(store.conn())?;
    let mut expired = Vec::new();
    for task in tasks.values() {
        if task.status != TaskStatus::InProgress || !owns(store, &task.ticket_key) {
            continue;
        }
        let Some(claim) = task.active_claim.as_ref() else {
            continue;
        };
        if claim.is_expired(now) {
            expired.push(ReapedClaim {
                ticket_key: task.ticket_key.clone(),
                agent_id: claim.agent_id.clone(),
                expired_at_epoch_ms: claim.lease_expires_at_epoch_ms,
            });
        }
    }
    expired.sort_by(|a, b| a.ticket_key.cmp(&b.ticket_key));
    Ok(expired)
}

/// Releases the lease and returns each task with an expired claim to `Ready`.
///
/// The release is recorded before the status change so the ledger reads in the order it happened: an
/// observer replaying it sees the lease end, then the task become available again.
///
/// # Errors
///
/// [`StorypoleError`] when the index cannot be read or an event cannot be recorded.
pub fn reap_expired_claims(store: &mut StorypoleStore) -> Result<Vec<ReapedClaim>, StorypoleError> {
    let now = StorypoleStore::now_ms();
    let tasks = get_all_tasks(store.conn())?;
    let lease_dir = crate::leases::dir_for_store(store.root_dir(), store.dot_dir());
    let mut reaped = Vec::new();

    for (task_id, task) in &tasks {
        if task.status != TaskStatus::InProgress || !owns(store, &task.ticket_key) {
            continue;
        }
        let Some(claim) = task.active_claim.as_ref() else {
            continue;
        };
        if !claim.is_expired(now) {
            continue;
        }

        // The shared lease goes with the branch-local one, or the ticket stays blocked in every other checkout.
        let _ = crate::leases::force_release(&lease_dir, &task.ticket_key);

        store.record_event(&LedgerEvent::TaskReleased {
            task_id: *task_id,
            agent_id: claim.agent_id.clone(),
            timestamp_ms: now,
        })?;
        store.record_event(&LedgerEvent::StatusChanged {
            task_id: *task_id,
            from: TaskStatus::InProgress,
            to: TaskStatus::Ready,
            reason: Some(format!(
                "claim lease expired at {} (agent {}); returned to the backlog",
                claim.lease_expires_at_epoch_ms, claim.agent_id
            )),
            timestamp_ms: now,
        })?;

        reaped.push(ReapedClaim {
            ticket_key: task.ticket_key.clone(),
            agent_id: claim.agent_id.clone(),
            expired_at_epoch_ms: claim.lease_expires_at_epoch_ms,
        });
    }

    reaped.sort_by(|a, b| a.ticket_key.cmp(&b.ticket_key));
    Ok(reaped)
}
