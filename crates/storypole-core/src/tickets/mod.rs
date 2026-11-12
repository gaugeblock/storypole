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

use std::fs;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use serde::{Deserialize, Serialize};

use crate::error::StorypoleError;
use crate::ledger::replay::replay_events;
use crate::model::{LedgerEvent, TaskNode};

pub mod io;
pub mod lock;
pub mod order;
pub mod refresh;
#[cfg(test)]
mod tests;

pub use io::{append_event, create, read, write};
pub use lock::TicketLock;
pub use order::{event_timestamp, order_events, task_id_of_event};
pub use refresh::{RefreshReport, refresh_store};

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
    /// The task as of its last event: a projection of `events`, refreshed on every write.
    ///
    /// It was documented as "the task as created", and that reading cost two wrong reports in one session
    /// (`GB-349`): a board count reading these blocks said 148 active where the store said 77, because an
    /// archive writes an event and the block kept the live status it was created with. Nothing is lost either
    /// way - `events` is the truth and [`TicketRecord::replay`] recovers it - but a field that reads like
    /// current state and is not is a trap for every reader that does not replay. It is current state now,
    /// kept true by [`write`] rather than by each caller remembering to.
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

/// Every record in the store, refusing a set that cannot be one truth.
///
/// Two checks, both of which `GB-79` earned the hard way: the file must be named for the ticket it holds,
/// and no task id may appear twice. A duplicate is corruption to report, because the alternative - pick
/// one and carry on - is how two live copies of a ticket came to disagree about whether it was done.
///
/// # Errors
///
/// [`StorypoleError::TicketRecord`] naming both paths when a name and its contents disagree, or when one
/// task id is stored twice.
pub fn read_all(dot_dir: &Path) -> Result<Vec<TicketRecord>, StorypoleError> {
    let dir = dir(dot_dir);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| StorypoleError::TicketRecord {
            path: dir.clone(),
            reason: e.to_string(),
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    paths.sort();

    let mut records = Vec::with_capacity(paths.len());
    let mut seen: std::collections::BTreeMap<Uuid, PathBuf> = std::collections::BTreeMap::new();
    for path in paths {
        let record = read(&path)?;
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if record.ticket.ticket_key != stem {
            return Err(StorypoleError::TicketRecord {
                path: path.clone(),
                reason: format!(
                    "file is named {stem} but holds {}; a record must be named for the ticket it stores",
                    record.ticket.ticket_key
                ),
            });
        }
        if let Some(first) = seen.insert(record.ticket.id, path.clone()) {
            return Err(StorypoleError::TicketRecord {
                path: path.clone(),
                reason: format!(
                    "task {} is stored twice: {} and {}",
                    record.ticket.id,
                    first.display(),
                    path.display()
                ),
            });
        }
        records.push(record);
    }
    Ok(records)
}
