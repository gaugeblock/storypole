//! Doing the cross-store repair and writing the links (`GB-347`).
//!
//! Order matters and is not an accident. The links are written from the plan, which was built before any repair: once a
//! citation is rewritten it names the ticket's current key, and the evidence that produced the link is gone. Repairing
//! first would silently drop every link the same run had just justified.
//!
//! A link is appended as an `edge_added` event, not only as an edge on the projection, because replay rebuilds the task
//! from the log - the same trap the remint itself had to learn.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde_json::{Value, json};
use storypole_core::store::StorypoleStore;
use storypole_core::ticket_keys::Remint;
use storypole_core::tickets::TicketRecord;
use uuid::Uuid;

use super::build;
use super::plan::{self, Link, Plan};

/// What the pass changed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Applied {
    /// Tickets whose text was repaired.
    pub tickets: usize,
    /// Citations rewritten.
    pub citations: usize,
    /// Edge targets rewritten because they named a dead key.
    pub edges: usize,
    /// Links written.
    pub links: usize,
}

/// Repairs every dead citation in the suite and writes the harness links.
///
/// # Errors
///
/// Returns an error when a ticket cannot be read or written, when a link cannot be written, or when the state afterwards
/// is not the state the plan promised - a repair that cannot prove itself has not finished.
pub fn apply(root: &Path, plan: &Plan) -> Result<Applied> {
    let mapping: Vec<Remint> = plan
        .moved
        .iter()
        .map(|(from, target)| Remint {
            from: from.clone(),
            to: target.key.clone(),
        })
        .collect();

    let mut applied = Applied::default();
    for (_, dir) in plan::stores(root) {
        for path in plan::ticket_files(&dir) {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("could not read {}", path.display()))?;
            let subset: Vec<Remint> = mapping
                .iter()
                .filter(|remint| text.contains(&remint.from))
                .cloned()
                .collect();
            if subset.is_empty() {
                continue;
            }
            let mut document: Value = serde_json::from_str(&text)
                .with_context(|| format!("{} is not valid JSON", path.display()))?;
            let (edges, citations) = plan::rewrite_document(&mut document, &subset);
            if edges + citations == 0 {
                continue;
            }
            std::fs::write(&path, serde_json::to_string_pretty(&document)?)
                .with_context(|| format!("could not write {}", path.display()))?;
            applied.tickets += 1;
            applied.edges += edges;
            applied.citations += citations;
        }
    }

    for link in &plan.links {
        if add_link(root, link)? {
            applied.links += 1;
        }
    }

    verify(root, plan)?;
    Ok(applied)
}

/// Appends a `related_to` edge to the harness ticket, returning whether one was added.
fn add_link(root: &Path, link: &Link) -> Result<bool> {
    let target_id = Uuid::parse_str(&link.target.id)
        .with_context(|| format!("{} is not a ticket uuid", link.target.id))?;
    let path = StorypoleStore::resolve_dot_dir(root)
        .join("tickets")
        .join(format!("{}.json", link.harness));
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("could not read {}", path.display()))?;
    let mut document: Value = serde_json::from_str(&text)
        .with_context(|| format!("{} is not valid JSON", path.display()))?;
    let task_id = document
        .get("ticket")
        .and_then(|ticket| ticket.get("id"))
        .and_then(Value::as_str)
        .and_then(|text| Uuid::parse_str(text).ok())
        .with_context(|| format!("{} carries no task id", path.display()))?;

    let edge = json!({
        "kind": "related_to",
        "target_id": target_id.to_string(),
        "target_ticket_key": link.target.key,
        "target_repo": link.target.store,
    });
    let event = json!({
        "event": "edge_added",
        "task_id": task_id.to_string(),
        "edge": edge,
        "timestamp_ms": now_ms(),
    });

    if let Some(ticket) = document.get_mut("ticket") {
        match ticket.get_mut("edges").and_then(Value::as_array_mut) {
            Some(edges) => edges.push(edge),
            None => ticket["edges"] = json!([edge]),
        }
    }
    match document.get_mut("events").and_then(Value::as_array_mut) {
        Some(events) => events.push(event),
        None => document["events"] = json!([event]),
    }
    std::fs::write(&path, serde_json::to_string_pretty(&document)?)
        .with_context(|| format!("could not write {}", path.display()))?;
    Ok(true)
}

/// Requires the store to be in the state the plan predicted: no dead citation left, every link present.
fn verify(root: &Path, plan: &Plan) -> Result<()> {
    let after = build::build(root)?;
    if !after.repairs.is_empty() {
        anyhow::bail!(
            "{} ticket(s) still cite a dead key after the repair",
            after.repairs.len()
        );
    }
    let mut missing = Vec::new();
    for link in &plan.links {
        if !has_edge(root, &link.harness, &link.target.id)? {
            missing.push(format!(
                "{} -> {} {}",
                link.harness, link.target.store, link.target.key
            ));
        }
    }
    if !missing.is_empty() {
        anyhow::bail!(
            "{} link(s) were not written:\n  {}",
            missing.len(),
            missing.join("\n  ")
        );
    }
    Ok(())
}

/// Whether the harness ticket's replayed state carries an edge to `target_id`.
fn has_edge(root: &Path, harness: &str, target_id: &str) -> Result<bool> {
    let path = StorypoleStore::resolve_dot_dir(root)
        .join("tickets")
        .join(format!("{harness}.json"));
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("could not read {}", path.display()))?;
    let record: TicketRecord = serde_json::from_str(&text)
        .with_context(|| format!("{} is not a replayable ticket record", path.display()))?;
    Ok(record.replay().is_some_and(|task| {
        task.edges
            .iter()
            .any(|edge| edge.target_id.to_string() == target_id)
    }))
}

/// Wall clock in epoch milliseconds, for the event the link is recorded as.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}
