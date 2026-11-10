//! Archive and unarchive command handlers.
//!
//! Both verbs are used in bulk from a board, which knows a ticket's key and nothing about which repository holds it. A
//! suite root's index lists member tickets, so resolving an id there is not evidence that the record is there: recording
//! the event in the wrong store failed with "no record for `<key>`" *after* the id resolved, which is how two tickets
//! survived a 262-ticket archive sweep (`GB-352`). Routing the write to the owning store is the fix.

use anyhow::{Context, Result};
use storypole_core::error::StorypoleError;
use storypole_core::index::query::{get_task_by_id, resolve_task_id};
use storypole_core::ops::archive::{archive_task, unarchive_task};
use storypole_core::store::StorypoleStore;
use storypole_core::workspace::lookup::{holds, locate_ticket, repo_root_of};
use uuid::Uuid;

/// Runs `action` against the store that owns `ident`, opening the owning store when the ticket lives elsewhere.
///
/// Returns the action's value and the repository it was performed in, when that was not the current one.
fn with_owner<T>(
    store: &mut StorypoleStore,
    ident: &str,
    action: impl FnOnce(&mut StorypoleStore, Uuid) -> Result<T, StorypoleError>,
) -> Result<(T, Option<String>)> {
    let id = resolve_task_id(store.conn(), ident)?;
    let key = get_task_by_id(store.conn(), id)?.ticket_key;
    if holds(store.dot_dir(), &key) {
        return Ok((action(store, id)?, None));
    }
    let dot = locate_ticket(store.root_dir(), &key)
        .with_context(|| format!("{key} is in the index, but no repository holds its record"))?;
    let repo = repo_root_of(&dot).to_path_buf();
    let mut owner = StorypoleStore::open_exact(&repo)
        .with_context(|| format!("could not open {}", repo.display()))?;
    let label = repo
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("another repository")
        .to_string();
    Ok((action(&mut owner, id)?, Some(label)))
}

pub fn handle_archive(store: &mut StorypoleStore, ident: &str, json: bool) -> Result<()> {
    let ((), elsewhere) = with_owner(store, ident, archive_task)?;
    if json {
        println!(
            "{}",
            serde_json::json!({ "success": true, "status": "archived", "repository": elsewhere })
        );
    } else if let Some(repo) = elsewhere {
        println!("Archived task {ident} in {repo}. Hidden from default backlog queries.");
    } else {
        println!("Archived task {ident}. Hidden from default backlog queries.");
    }
    Ok(())
}

pub fn handle_unarchive(store: &mut StorypoleStore, ident: &str, json: bool) -> Result<()> {
    let (status, elsewhere) = with_owner(store, ident, unarchive_task)?;
    if json {
        println!(
            "{}",
            serde_json::json!({ "success": true, "status": status, "repository": elsewhere })
        );
    } else if let Some(repo) = elsewhere {
        println!("Restored task {ident} in {repo} to active status: {status:?}");
    } else {
        println!("Restored task {ident} to active status: {status:?}");
    }
    Ok(())
}
