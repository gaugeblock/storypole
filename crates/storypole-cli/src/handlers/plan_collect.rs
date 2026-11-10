//! Collecting a blueprint into the store that owns its ticket (`GB-350`).
//!
//! Blueprints were written to the canonical root plans directory while the tickets were distributed into members, so
//! neither end of the relationship holds: the file sits where no ticket can reach it, and the ticket's pointer resolves
//! to nothing.
//!
//! The destination needs no decision. The pointer already names the ticket's current key inside its own store, so moving
//! the blueprint there makes the pointer true as it stands. The source needs one lookup: the file on disk is named for
//! the key the ticket carried *before* its remint, which the store's remint record supplies by uuid.
//!
//! A blueprint whose source cannot be found is reported, never invented. Moving the wrong document onto a ticket is
//! worse than leaving a pointer dangling, because the second is visible and the first is not.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use storypole_core::store::StorypoleStore;
use storypole_core::tickets::TicketRecord;

use crate::handlers::relink::plan::{stores, ticket_files};
use crate::handlers::remint::record;

/// One blueprint to move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Move {
    /// The ticket whose pointer the move makes true.
    pub ticket: String,
    /// The store it belongs to.
    pub store: String,
    /// Where the blueprint is now.
    pub from: PathBuf,
    /// Where the ticket says it should be.
    pub to: PathBuf,
}

/// What collecting would do, and what it cannot place.
#[derive(Debug, Default)]
pub struct Plan {
    /// Blueprints to move.
    pub moves: Vec<Move>,
    /// Pointers whose blueprint was not found, named rather than guessed at.
    pub unresolved: Vec<String>,
}

/// Finds every pointer whose blueprint exists in the canonical plans directory.
///
/// # Errors
///
/// Returns an error when a ticket or a remint record cannot be read, because a move computed from a partial view could
/// place a file against the wrong ticket.
pub fn build(root: &Path) -> Result<Plan> {
    let canonical = StorypoleStore::resolve_dot_dir(root).join("plans");
    let mut plan = Plan::default();
    let mut claimed: BTreeMap<PathBuf, String> = BTreeMap::new();

    for (name, dir) in stores(root) {
        let previous: BTreeMap<String, String> = record::load(&dir)?
            .into_iter()
            .map(|entry| (entry.id.to_string(), entry.from))
            .collect();
        for path in ticket_files(&dir) {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("could not read {}", path.display()))?;
            let record: TicketRecord = serde_json::from_str(&text)
                .with_context(|| format!("{} is not a replayable ticket record", path.display()))?;
            let Some(task) = record.replay() else {
                continue;
            };
            let Some(pointer) = task.plan.and_then(|plan| plan.plan_file_path) else {
                continue;
            };
            if dir.join(&pointer).exists() {
                continue;
            }
            let label = if name == "." {
                task.ticket_key.clone()
            } else {
                format!("{name}/{}", task.ticket_key)
            };
            // A blueprint belongs in its store's canonical plans directory. A pointer naming anywhere else is a
            // different defect: three tickets still point at the pre-`GB-344` `.storypole/plans/` layout, and moving the
            // file to satisfy that would resurrect the legacy store the migration removed instead of fixing the pointer.
            let canonical_target = StorypoleStore::resolve_dot_dir(&dir)
                .join("plans")
                .join(format!("{}.md", task.ticket_key));
            if dir.join(&pointer) != canonical_target {
                plan.unresolved.push(format!(
                    "{label} -> {pointer} names a non-canonical location; the pointer needs correcting, not the file"
                ));
                continue;
            }
            let stem = Path::new(&pointer)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_string();
            let source = previous
                .get(&record.ticket.id.to_string())
                .map(|old| canonical.join(format!("{old}.md")))
                .filter(|candidate| candidate.exists())
                .or_else(|| {
                    let same = canonical.join(format!("{stem}.md"));
                    same.exists().then_some(same)
                });

            match source {
                Some(from) => {
                    if let Some(other) = claimed.insert(from.clone(), label.clone()) {
                        plan.unresolved
                            .push(format!("{label} and {other} both claim {}", from.display()));
                        continue;
                    }
                    plan.moves.push(Move {
                        ticket: task.ticket_key.clone(),
                        store: name.clone(),
                        from,
                        to: canonical_target.clone(),
                    });
                }
                None => plan
                    .unresolved
                    .push(format!("{label} -> {pointer} has no blueprint")),
            }
        }
    }
    plan.moves.sort_by(|left, right| left.to.cmp(&right.to));
    plan.unresolved.sort();
    Ok(plan)
}

/// Moves every blueprint in `plan`, then proves each pointer resolves.
///
/// # Errors
///
/// Returns an error when a blueprint cannot be moved, or when any pointer still does not resolve afterwards.
pub fn apply(plan: &Plan) -> Result<usize> {
    for entry in &plan.moves {
        if let Some(parent) = entry.to.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        std::fs::rename(&entry.from, &entry.to).with_context(|| {
            format!(
                "could not move {} to {}",
                entry.from.display(),
                entry.to.display()
            )
        })?;
    }
    for entry in &plan.moves {
        if !entry.to.exists() {
            anyhow::bail!("{} does not resolve after the move", entry.to.display());
        }
    }
    Ok(plan.moves.len())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use gaugeblock_contracts::storypole::TaskPlan;
    use storypole_core::model::LedgerEvent;

    use crate::handlers::remint::test_support::task;

    /// A ticket whose plan pointer names `pointer`.
    fn with_plan(key: &str, pointer: &str) -> TicketRecord {
        let mut created = task(key);
        created.plan = Some(TaskPlan {
            summary: "a plan".to_string(),
            design_markdown: String::new(),
            steps: Vec::new(),
            plan_file_path: Some(pointer.to_string()),
            updated_at_epoch_ms: 1,
        });
        TicketRecord::new(
            created.clone(),
            vec![LedgerEvent::TaskCreated { task: created }],
        )
    }

    #[test]
    fn a_blueprint_in_the_canonical_directory_is_moved_to_its_ticket() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let member_tickets = root.join("member/.gaugeblock/storypole/tickets");
        std::fs::create_dir_all(&member_tickets).unwrap();
        let record = with_plan("OF-1", ".gaugeblock/storypole/plans/OF-1.md");
        std::fs::write(
            member_tickets.join("OF-1.json"),
            serde_json::to_string(&record).unwrap(),
        )
        .unwrap();

        // The blueprint is where the distribution left it: in the canonical directory, under the old key.
        let canonical = root.join(".gaugeblock/storypole/plans");
        std::fs::create_dir_all(&canonical).unwrap();
        std::fs::write(canonical.join("GB-9.md"), "# the plan").unwrap();
        record::append(
            &root.join("member"),
            &[record::AppliedRemint {
                id: record.ticket.id,
                from: "GB-9".to_string(),
                to: "OF-1".to_string(),
            }],
        )
        .unwrap();

        let planned = build(root).unwrap();
        assert_eq!(planned.moves.len(), 1, "one blueprint to place");
        assert_eq!(planned.moves[0].ticket, "OF-1");
        assert!(planned.unresolved.is_empty());

        assert_eq!(apply(&planned).unwrap(), 1);
        assert!(
            root.join("member/.gaugeblock/storypole/plans/OF-1.md")
                .exists()
        );
        assert!(
            build(root).unwrap().moves.is_empty(),
            "the pointer resolves now"
        );
    }

    #[test]
    fn a_pointer_with_no_blueprint_is_reported_rather_than_invented() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let tickets = root.join("member/.gaugeblock/storypole/tickets");
        std::fs::create_dir_all(&tickets).unwrap();
        let record = with_plan("OF-1", ".gaugeblock/storypole/plans/OF-1.md");
        std::fs::write(
            tickets.join("OF-1.json"),
            serde_json::to_string(&record).unwrap(),
        )
        .unwrap();

        let planned = build(root).unwrap();
        assert!(planned.moves.is_empty(), "nothing to move");
        assert_eq!(planned.unresolved.len(), 1, "and it says so");
    }
}
