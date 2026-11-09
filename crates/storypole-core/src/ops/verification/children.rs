//! Refusing to close an epic while its children are open (`GB-334`).
//!
//! An epic is a promise that a body of work is finished. Closing it with open children produces exactly the record
//! that misled us before: a green status over unfinished work, and a reader who has no reason to doubt it.
//!
//! Children are discovered through the edges they carry - `storypole link CHILD EPIC --kind parent_epic` records the
//! relationship on the child - so this asks the store for every task that points at the epic, not the epic for a
//! list it might have lost.
//!
//! **Scope:** only children in the same store are visible here. Since ADR 0009 the rest live in other repositories,
//! and seeing those needs the workspace rollup. The refusal is therefore honest about what it can prove rather than
//! silent about what it cannot: it refuses on the evidence it has and does not claim to have checked the fleet.

use std::collections::BTreeMap;

use gaugeblock_contracts::{EdgeKind, TaskKind, TaskNode, TaskStatus};
use rusqlite::Connection;
use uuid::Uuid;

use crate::error::StorypoleError;
use crate::index::query::get_all_tasks;

/// Refuses verification when a child task in this store is not finished.
///
/// # Errors
///
/// [`StorypoleError::EpicHasOpenChildren`] naming every open child, because "epic has children" without the list
/// tells the reader nothing they can act on.
pub fn unresolved_children(conn: &Connection, epic: &TaskNode) -> Result<(), StorypoleError> {
    if epic.kind != TaskKind::Epic {
        return Ok(());
    }
    let tasks: BTreeMap<Uuid, TaskNode> = get_all_tasks(conn)?;
    let open: Vec<String> = tasks
        .values()
        .filter(|candidate| candidate.id != epic.id)
        .filter(|candidate| points_at(candidate, epic.id))
        .filter(|candidate| !is_finished(candidate))
        .map(|candidate| format!("{} ({:?})", candidate.ticket_key, candidate.status))
        .collect();
    if open.is_empty() {
        return Ok(());
    }
    Err(StorypoleError::EpicHasOpenChildren {
        ticket: epic.ticket_key.clone(),
        children: open,
    })
}

/// Whether this task declares `epic_id` as its parent.
fn points_at(candidate: &TaskNode, epic_id: Uuid) -> bool {
    candidate
        .edges
        .iter()
        .any(|edge| edge.kind == EdgeKind::ParentEpic && edge.target_id == epic_id)
}

/// A child is finished when it is verified, or archived after being verified.
///
/// Archived is a state rather than a status (so a verified task keeps reporting `Verified`), and a task retired
/// without verification is not "open" either - it was dealt with, whatever the outcome.
fn is_finished(candidate: &TaskNode) -> bool {
    candidate.status == TaskStatus::Verified || candidate.archived_at_epoch_ms.is_some()
}
