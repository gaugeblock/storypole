//! One file per ticket (`GB-329`).
//!
//! The suite stored tickets as one append-only ledger per repository. That made every write a write to
//! the most contended file in the repository: two branches appending at end-of-file always conflict, an
//! ownership move is surgery on a log, and a ticket's mapping to an external tracker has nowhere to live.
//!
//! Here a ticket is a file. `tickets/<KEY>.json` holds the task, its ordered events and an optional
//! external reference, so a merge is a file merge, an ownership move is a file move, and two branches
//! touching different tickets never meet.
//!
//! Two invariants, both paid for:
//!
//! * **One file per ticket.** Identity is the task uuid, and the same uuid in two files is corruption to
//!   report rather than state to merge - which is exactly how `GB-79` diverged.
//! * **Order comes from the record, not the file.** Events replay by `(timestamp, id)`, so a merge that
//!   interleaves two histories cannot let a stale status win on line position.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::StorypoleError;
use crate::ledger::replay::replay_events;
use crate::model::{LedgerEvent, TaskNode};

pub mod io;
pub mod lock;
pub mod order;
#[cfg(test)]
mod tests;

pub use io::{append_event, read, write};
pub use lock::TicketLock;
pub use order::{event_timestamp, order_events};

/// Mapping from a ticket to its row in an external tracker.
///
/// A field on the record rather than a table elsewhere, because the mapping must travel with the ticket
/// it describes: a central mapping would itself be cross-repository state, and a per-ticket sync failure
/// stays isolated to one file. No adapter is implemented - this is the seam (`GB-339`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalRef {
    /// Adapter name: `github`, `jira`, `gitlab`.
    pub system: String,
    /// Identifier in that system; a string because not every tracker uses numbers.
    pub id: String,
    /// When the record was last reconciled, epoch milliseconds.
    pub synced_at_ms: u64,
}

/// A ticket and everything that has happened to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TicketRecord {
    /// The task as created; later events carry every change.
    pub ticket: TaskNode,
    /// Every event for this ticket, stored already ordered.
    pub events: Vec<LedgerEvent>,
    /// External tracker mapping, when the ticket is mirrored elsewhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<ExternalRef>,
}

impl TicketRecord {
    /// Builds a record from the task and its events, ordering them before they are written.
    #[must_use]
    pub fn new(ticket: TaskNode, mut events: Vec<LedgerEvent>) -> Self {
        order_events(&mut events);
        Self {
            ticket,
            events,
            external: None,
        }
    }

    /// The task as of its last event.
    ///
    /// `None` only when the record holds no event naming this ticket, which a written record cannot do:
    /// it is the ticket's own file.
    #[must_use]
    pub fn replay(&self) -> Option<TaskNode> {
        let mut tasks = replay_events(&self.events);
        tasks.remove(&self.ticket.id)
    }
}

/// The directory holding ticket records for a store.
#[must_use]
pub fn dir(dot_dir: &Path) -> PathBuf {
    dot_dir.join("tickets")
}

/// Path of one ticket's record, refusing a key that could escape the directory.
///
/// # Errors
///
/// [`StorypoleError::TicketRecord`] when the key is not a bare file name.
pub fn path(dot_dir: &Path, key: &str) -> Result<PathBuf, StorypoleError> {
    if key.is_empty() || key.contains('/') || key.contains('\\') || key.contains("..") {
        return Err(StorypoleError::TicketRecord {
            path: dir(dot_dir),
            reason: format!("'{key}' is not a usable ticket key"),
        });
    }
    Ok(dir(dot_dir).join(format!("{key}.json")))
}
