//! Applying a migration plan (`GB-331`).
//!
//! Three phases, in this order for one reason: the ledger is the only copy of the history until the records are
//! proved equivalent, so it is archived last and only after a differential replay agrees.
//!
//! 1. **write** every ticket as a record in its destination store;
//! 2. **verify** by replaying each destination and comparing task states with the ledger's;
//! 3. **archive** the source ledger, so a failed verification leaves the original untouched and the cost is a
//!    few extra files that can be deleted.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::error::StorypoleError;
use crate::model::{LedgerEvent, TaskNode};
use crate::state::{self, LoadedState};
use crate::tickets::{self, TicketRecord};

use super::{Destination, Plan};

/// What a completed migration did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// Tickets written as records.
    pub written: usize,
    /// Of those, how many went to a member repository.
    pub moved: usize,
    /// Where the original ledger was preserved.
    pub archive: PathBuf,
}

/// Writes the plan, proves it, then archives the source ledger.
///
/// `store_for` maps a repository name to the storypole directory of its store. Returning `None` for a planned
/// destination is an error rather than a skip: a ticket that cannot be written must not slide out of the plan.
///
/// # Errors
///
/// [`StorypoleError::TicketRecord`] when a record cannot be written or a destination is unknown, and
/// [`StorypoleError::MigrationDivergence`] when replaying the records does not reproduce the ledger's tasks.
pub fn apply(
    source_dot: &Path,
    state: &LoadedState,
    plan: &Plan,
    store_for: &dyn Fn(&str) -> Option<PathBuf>,
) -> Result<Applied, StorypoleError> {
    let mut events_by_task: BTreeMap<Uuid, Vec<LedgerEvent>> = BTreeMap::new();
    for event in &state.events {
        events_by_task
            .entry(tickets::task_id_of_event(event))
            .or_default()
            .push(event.clone());
    }

    let mut written = 0;
    let mut moved = 0;
    let mut destinations: BTreeSet<PathBuf> = BTreeSet::new();

    for assignment in &plan.assignments {
        let Some(task) = state.tasks.get(&assignment.task_id) else {
            continue;
        };
        let destination = destination_of(source_dot, &assignment.destination, store_for)?;
        let events = events_by_task
            .get(&assignment.task_id)
            .cloned()
            .unwrap_or_default();
        if events.is_empty() {
            return Err(StorypoleError::TicketRecord {
                path: source_dot.to_path_buf(),
                reason: format!(
                    "{} has a task but no events; writing it would invent history",
                    assignment.ticket_key
                ),
            });
        }
        let record = TicketRecord::new(task.clone(), events);
        tickets::write(
            &tickets::path(&destination, &assignment.ticket_key)?,
            &record,
        )?;
        destinations.insert(destination);
        written += 1;
        if matches!(assignment.destination, Destination::Member(_)) {
            moved += 1;
        }
    }

    // Phase 2: the new records must reproduce the ledger's tasks, or nothing else happens.
    verify(state, &destinations)?;

    // Phase 3: only now is the original superseded.
    let archive = archive_ledger(source_dot)?;
    Ok(Applied {
        written,
        moved,
        archive,
    })
}

fn destination_of(
    source_dot: &Path,
    destination: &Destination,
    store_for: &dyn Fn(&str) -> Option<PathBuf>,
) -> Result<PathBuf, StorypoleError> {
    match destination {
        Destination::Root => Ok(source_dot.to_path_buf()),
        Destination::Member(repo) => store_for(repo).ok_or_else(|| StorypoleError::TicketRecord {
            path: source_dot.to_path_buf(),
            reason: format!(
                "a ticket is routed to '{repo}', which has no store; nothing was archived"
            ),
        }),
    }
}

/// Replays every destination and compares it with what the ledger said.
///
/// The comparison is over fields a reader would see, not over event bytes: the migration is allowed to change
/// how history is stored, and is not allowed to change what it says.
fn verify(state: &LoadedState, destinations: &BTreeSet<PathBuf>) -> Result<(), StorypoleError> {
    let mut wanted: BTreeMap<Uuid, String> = BTreeMap::new();
    for task in state.tasks.values() {
        wanted.insert(task.id, fingerprint(task));
    }

    for destination in destinations {
        let loaded = state::load(destination)?;
        for (id, task) in &loaded.tasks {
            match wanted.get(id) {
                Some(expected) if expected == &fingerprint(task) => {}
                Some(expected) => {
                    return Err(StorypoleError::MigrationDivergence {
                        ticket: task.ticket_key.clone(),
                        detail: format!(
                            "replayed as '{expected}' from the ledger, but {} now replays as '{}'",
                            destination.display(),
                            fingerprint(task)
                        ),
                    });
                }
                None => {
                    return Err(StorypoleError::MigrationDivergence {
                        ticket: task.ticket_key.clone(),
                        detail: format!(
                            "{} holds task {id}, which the ledger did not",
                            destination.display()
                        ),
                    });
                }
            }
        }
    }

    // And nothing may go missing: every task the ledger had must be present in some destination.
    let mut replayed: BTreeSet<Uuid> = BTreeSet::new();
    for destination in destinations {
        replayed.extend(state::load(destination)?.tasks.keys().copied());
    }
    for (id, task) in &state.tasks {
        if !replayed.contains(id) {
            return Err(StorypoleError::MigrationDivergence {
                ticket: task.ticket_key.clone(),
                detail: format!("task {id} was not written to any destination"),
            });
        }
    }
    Ok(())
}

/// How a task is compared: the fields a reader of the store would see.
fn fingerprint(task: &TaskNode) -> String {
    format!(
        "{}|{:?}|{}|{}|{}|{:?}|{}|{}",
        task.ticket_key,
        task.status,
        task.priority,
        task.title,
        task.description.len(),
        task.scope.repo,
        task.edges.len(),
        task.archived_at_epoch_ms.is_some()
    )
}

/// Moves the source ledger out of the way, keeping it as the record of what was migrated.
fn archive_ledger(source_dot: &Path) -> Result<PathBuf, StorypoleError> {
    let ledger = source_dot.join("ledger.jsonl");
    let archive_dir = source_dot.join("ledger-archive");
    let archive = archive_dir.join("ledger-migrated.jsonl");
    if !ledger.is_file() {
        return Ok(archive);
    }
    fs::create_dir_all(&archive_dir).map_err(|e| StorypoleError::IoWithPath {
        path: archive_dir.clone(),
        source: e,
    })?;
    fs::rename(&ledger, &archive).map_err(|e| StorypoleError::IoWithPath {
        path: ledger,
        source: e,
    })?;
    Ok(archive)
}

#[cfg(test)]
mod tests;
