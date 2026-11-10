//! Proving a remint against replay rather than against the file it just wrote (`GB-347` step 4).
//!
//! The check that shipped with the first pass re-listed the store's keys and confirmed they carried the prefix. It
//! could not fail for the reason that mattered: the key was right and the description beside it still named the old
//! one, so eleven stores would have "verified" with their served state citing keys that no longer existed.
//!
//! This replays every record through the same code the tool uses, then refuses if a moved key appears anywhere in the
//! resulting task. It is deliberately blind to which field the key sat in, so it cannot be fooled by a field the
//! rewrite pass does not know about - which is how the defect was found in the first place.

use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;
use storypole_core::ticket_keys::Remint;
use storypole_core::tickets::TicketRecord;

use crate::handlers::remint_store::store_tickets;

use super::rewrite::cites;

/// Refuses when the replayed state of any ticket in `store` still cites a key the remint moved.
///
/// # Errors
///
/// Returns an error when a record cannot be read, parsed or replayed, or when any moved key survives in the served
/// state of a ticket. Both are failures to report rather than to skip: a record that cannot be replayed cannot be
/// certified, and one that still cites an old key is the defect this check exists for.
pub(crate) fn verify_replay(store: &Path, mapping: &[Remint]) -> Result<()> {
    if mapping.is_empty() {
        return Ok(());
    }
    let mut problems = Vec::new();
    for path in store_tickets(store) {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("could not read {}", path.display()))?;
        let record: TicketRecord = serde_json::from_str(&text)
            .with_context(|| format!("{} is not a replayable ticket record", path.display()))?;
        let Some(task) = record.replay() else {
            // Skipping would be the same mistake this check exists to catch: a record whose replay yields nothing
            // cannot be certified, and silence would report it as clean.
            problems.push(format!(
                "{}: no event names this ticket, so its served state cannot be replayed",
                path.display()
            ));
            continue;
        };
        let served = serde_json::to_value(&task).with_context(|| {
            format!(
                "could not serialise the replay of {}",
                record.ticket.ticket_key
            )
        })?;
        for remint in mapping {
            if let Some(field) = find_citation(&served, &remint.from, "") {
                problems.push(format!(
                    "{}: replay still serves {} at {field}, should be {}",
                    record.ticket.ticket_key, remint.from, remint.to
                ));
            }
        }
    }
    if !problems.is_empty() {
        anyhow::bail!(
            "{} ticket(s) still cite a moved key after the remint:\n  {}",
            problems.len(),
            problems.join("\n  ")
        );
    }
    Ok(())
}

/// The first field of `value` that cites `key`, named by path, or `None` when nothing does.
fn find_citation(value: &Value, key: &str, path: &str) -> Option<String> {
    match value {
        Value::String(text) => cites(text, key).then(|| path.to_string()),
        Value::Object(map) => map.iter().find_map(|(name, child)| {
            let field = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}.{name}")
            };
            // A claim's branch names the branch the work was done on. Renaming it would invent a branch that never
            // existed, so it is history and is exempt (`GB-347`, decision 2026-11-10).
            if field == "active_claim.branch" {
                None
            } else {
                find_citation(child, key, &field)
            }
        }),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(index, child)| find_citation(child, key, &format!("{path}[{index}]"))),
        _ => None,
    }
}
