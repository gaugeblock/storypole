//! Computing the cross-store plan, and the links it justifies (`GB-347`).
//!
//! Split from `plan` so the types a caller reads stay in one screen and the traversal stays in another; the verb reports
//! a plan, applies it, then rebuilds it to prove nothing is left.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;
use storypole_core::ticket_keys::Remint;
use storypole_core::tickets::TicketRecord;

use crate::handlers::remint::record;
use crate::handlers::remint::rewrite::cites_bare;

use super::plan::{
    Link, Plan, Repair, Target, rewrite_document, stores, ticket_files, ticket_keys,
};

/// Builds the plan for the suite rooted at `root`.
///
/// # Errors
///
/// Returns an error when a store's remint record or a ticket file cannot be read, because a repair computed from a
/// partial view would certify a store it never looked at.
pub fn build(root: &Path) -> Result<Plan> {
    let stores = stores(root);
    let alive: BTreeSet<String> = stores
        .iter()
        .flat_map(|(_, dir)| ticket_keys(dir))
        .collect();

    let mut plan = Plan::default();
    for (name, dir) in &stores {
        for entry in record::load(dir)? {
            if alive.contains(&entry.from) {
                continue;
            }
            let target = Target {
                store: name.clone(),
                key: entry.to.clone(),
                id: entry.id.to_string(),
            };
            match plan.moved.get(&entry.from) {
                Some(known) if known != &target => {
                    plan.conflicts
                        .push(format!("{} -> {} or {}", entry.from, known.key, target.key));
                    plan.moved.remove(&entry.from);
                }
                Some(_) => {}
                None => {
                    plan.moved.insert(entry.from.clone(), target);
                }
            }
        }
    }

    let mapping: Vec<Remint> = plan
        .moved
        .iter()
        .map(|(from, target)| Remint {
            from: from.clone(),
            to: target.key.clone(),
        })
        .collect();

    for (name, dir) in &stores {
        for path in ticket_files(dir) {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("could not read {}", path.display()))?;
            let subset: Vec<Remint> = mapping
                .iter()
                .filter(|remint| cites_bare(&text, &remint.from))
                .cloned()
                .collect();
            if subset.is_empty() {
                continue;
            }
            let mut document: Value = serde_json::from_str(&text)
                .with_context(|| format!("{} is not valid JSON", path.display()))?;
            let key = document
                .get("ticket")
                .and_then(|ticket| ticket.get("ticket_key"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let (edges, citations) = rewrite_document(&mut document, &subset);
            if edges + citations > 0 {
                plan.repairs.push(Repair {
                    store: name.clone(),
                    key,
                });
            }
        }
    }

    plan.links = links(root, &plan.moved)?;
    Ok(plan)
}

/// Harness tickets that name a member ticket in their description and do not yet link to it.
///
/// The description is the field that states what the ticket is about, so a key cited there is evidence that the member
/// ticket carries the work. A mention anywhere else is a reference, not a claim of ownership.
fn links(root: &Path, moved: &BTreeMap<String, Target>) -> Result<Vec<Link>> {
    let mut found: Vec<Link> = Vec::new();
    for path in ticket_files(root) {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("could not read {}", path.display()))?;
        let record: TicketRecord = serde_json::from_str(&text)
            .with_context(|| format!("{} is not a replayable ticket record", path.display()))?;
        let Some(task) = record.replay() else {
            continue;
        };
        for (key, target) in moved {
            if target.store == "." || !cites_bare(&task.description, key) {
                continue;
            }
            if task
                .edges
                .iter()
                .any(|edge| edge.target_id.to_string() == target.id)
            {
                continue;
            }
            let known = found
                .iter()
                .any(|link| link.harness == task.ticket_key && link.target.id == target.id);
            if !known {
                found.push(Link {
                    harness: task.ticket_key.clone(),
                    target: target.clone(),
                });
            }
        }
    }
    Ok(found)
}
