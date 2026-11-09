//! Writing events to ticket records instead of the append-only ledger (`GB-329`).
//!
//! The store decides *which* layout is authoritative; this module knows how to write the record one. It is
//! separate from `store.rs` for the usual reason - that file is close to its line limit - but also because
//! resolving an event to a ticket is the only piece that needs the index, and keeping it here makes that
//! dependency visible.

use std::path::Path;

use rusqlite::{Connection, params};

use crate::error::StorypoleError;
use crate::model::LedgerEvent;
use crate::tickets;

/// Appends one event to the record that owns it.
///
/// The owning key comes from the event itself when it carries a whole task, and from the index otherwise:
/// a ticket created now is not in the index yet, while every later event names only its id.
///
/// # Errors
///
/// [`StorypoleError::TicketRecord`] when the event names a task no record holds, or when the record cannot
/// be read or written.
pub fn record_event(
    conn: &Connection,
    dot_dir: &Path,
    event: &LedgerEvent,
) -> Result<(), StorypoleError> {
    let key = owning_key(conn, dot_dir, event)?;
    tickets::append_event(dot_dir, &key, event)?;
    Ok(())
}

/// The ticket key an event belongs to.
fn owning_key(
    conn: &Connection,
    dot_dir: &Path,
    event: &LedgerEvent,
) -> Result<String, StorypoleError> {
    if let Some(key) = carried_key(event) {
        return Ok(key.to_string());
    }
    let id = tickets::task_id_of_event(event).to_string();
    conn.query_row(
        "SELECT ticket_key FROM tasks WHERE id = ?",
        params![id],
        |row| row.get::<_, String>(0),
    )
    .map_err(|_| StorypoleError::TicketRecord {
        path: tickets::dir(dot_dir),
        reason: format!("event targets task {id}, which no record holds"),
    })
}

/// The key an event carries itself, which only the two whole-task variants do.
fn carried_key(event: &LedgerEvent) -> Option<&str> {
    match event {
        LedgerEvent::TaskCreated { task } | LedgerEvent::DeferredItemCaptured { task, .. } => {
            Some(task.ticket_key.as_str())
        }
        _ => None,
    }
}
