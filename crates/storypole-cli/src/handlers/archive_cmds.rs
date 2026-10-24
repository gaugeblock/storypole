//! Archive and unarchive command handlers.

use anyhow::Result;
use storypole_core::index::query::resolve_task_id;
use storypole_core::ops::archive::{archive_task, unarchive_task};
use storypole_core::store::StorypoleStore;

pub fn handle_archive(store: &mut StorypoleStore, ident: &str, json: bool) -> Result<()> {
    let id = resolve_task_id(store.conn(), ident)?;
    archive_task(store, id)?;
    if json {
        println!("{{\"success\":true,\"status\":\"archived\"}}");
    } else {
        println!("Archived task {ident}. Hidden from default backlog queries.");
    }
    Ok(())
}

pub fn handle_unarchive(store: &mut StorypoleStore, ident: &str, json: bool) -> Result<()> {
    let id = resolve_task_id(store.conn(), ident)?;
    let status = unarchive_task(store, id)?;
    if json {
        println!("{{\"success\":true,\"status\":\"{:?}\"}}", status);
    } else {
        println!("Restored task {ident} to active status: {status:?}");
    }
    Ok(())
}
