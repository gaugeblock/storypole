//! Handlers for the deliberate status transitions: park, unpark, reopen (`SP-47`).

use crate::json_out::print_json;
use anyhow::Result;
use storypole_core::index::query::resolve_task_id;
use storypole_core::model::task::TaskStatus;
use storypole_core::ops::lifecycle::{park_task, reopen_task, unpark_task};
use storypole_core::store::StorypoleStore;

/// `snake_case` name of a status, so `--json` and the human line agree with the ledger.
fn wire(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Triage => "triage",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Ready => "ready",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Synthesizing => "synthesizing",
        TaskStatus::Verifying => "verifying",
        TaskStatus::Verified => "verified",
        TaskStatus::Deferred => "deferred",
        TaskStatus::Archived => "archived",
        TaskStatus::Failed => "failed",
    }
}

/// Parks a task so it leaves the ready queue without being closed.
pub fn handle_park(
    store: &mut StorypoleStore,
    ident: &str,
    reason: Option<&str>,
    json: bool,
) -> Result<()> {
    let id = resolve_task_id(store.conn(), ident)?;
    let status = park_task(store, id, reason)?;
    if json {
        print_json(
            "park",
            &serde_json::json!({"ticket": ident, "status": wire(status)}),
        )?;
    } else {
        println!(
            "Parked {ident} ({}). It stays in the backlog and leaves the ready queue, so \
             `storypole next` will not offer it.",
            wire(status)
        );
    }
    Ok(())
}

/// Returns a parked task to the backlog.
pub fn handle_unpark(store: &mut StorypoleStore, ident: &str, json: bool) -> Result<()> {
    let id = resolve_task_id(store.conn(), ident)?;
    let status = unpark_task(store, id)?;
    if json {
        print_json(
            "unpark",
            &serde_json::json!({"ticket": ident, "status": wire(status)}),
        )?;
    } else if status == TaskStatus::Blocked {
        println!(
            "Returned {ident} to the backlog as blocked: a task it depends on is not verified yet, \
             so it is not offered as ready work."
        );
    } else {
        println!("Returned {ident} to the backlog ({}).", wire(status));
    }
    Ok(())
}

/// Reopens a verified task, keeping the receipt that certified it.
pub fn handle_reopen(
    store: &mut StorypoleStore,
    ident: &str,
    reason: &str,
    json: bool,
) -> Result<()> {
    let id = resolve_task_id(store.conn(), ident)?;
    let status = reopen_task(store, id, reason)?;
    if json {
        print_json(
            "reopen",
            &serde_json::json!({"ticket": ident, "status": wire(status), "reason": reason}),
        )?;
    } else {
        println!(
            "Reopened {ident} ({}). The receipt that certified it is kept, so the record still shows \
             what was verified and when.",
            wire(status)
        );
    }
    Ok(())
}
