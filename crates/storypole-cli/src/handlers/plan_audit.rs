//! Plan blueprints and the tickets that name them (`GB-350`).
//!
//! A plan is a file named for its ticket, and the ticket carries a pointer to it. Neither end is checked against the
//! other, so a store can hold a blueprint nothing owns and a ticket can point at a file that was never written - which is
//! what a remint found in three stores while migrating everything else.
//!
//! This audits and **reports only**: it does not gate the exit code. Resolving an orphan means deciding which ticket the
//! blueprint was meant for, and a check that guessed would be worse than one that asks.

use std::path::Path;

use anyhow::{Context, Result};
use storypole_core::store::StorypoleStore;
use storypole_core::tickets::TicketRecord;

use crate::handlers::relink::plan::{stores, ticket_files};

/// What a store's plans look like next to its tickets.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PlanAudit {
    /// Blueprints with no ticket of that key.
    pub orphans: Vec<String>,
    /// `ticket -> path` where the path does not exist.
    pub dangling: Vec<String>,
}

impl PlanAudit {
    /// Whether the plans and the tickets agree.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.orphans.is_empty() && self.dangling.is_empty()
    }
}

/// Audits every store under `root`, including the root's own.
///
/// # Errors
///
/// Returns an error when a ticket cannot be read, because an audit that skips an unreadable ticket would report a store
/// as clean without having looked at it.
pub fn audit(root: &Path) -> Result<PlanAudit> {
    let mut audit = PlanAudit::default();
    for (name, dir) in stores(root) {
        let label = if name == "." {
            String::new()
        } else {
            format!("{name}/")
        };
        let plans = StorypoleStore::resolve_dot_dir(&dir).join("plans");

        let tickets: Vec<_> = ticket_files(&dir);
        let mut owned: Vec<String> = Vec::new();
        for path in &tickets {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("could not read {}", path.display()))?;
            let record: TicketRecord = serde_json::from_str(&text)
                .with_context(|| format!("{} is not a replayable ticket record", path.display()))?;
            owned.push(record.ticket.ticket_key.clone());

            let pointer = record
                .replay()
                .and_then(|task| task.plan)
                .and_then(|plan| plan.plan_file_path);
            if let Some(pointer) = pointer
                && !dir.join(&pointer).exists()
            {
                audit
                    .dangling
                    .push(format!("{label}{} -> {pointer}", record.ticket.ticket_key));
            }
        }

        if let Ok(entries) = std::fs::read_dir(&plans) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "md") {
                    let stem = path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or_default()
                        .to_string();
                    if !owned.contains(&stem) {
                        audit
                            .orphans
                            .push(format!("{label}plans/{stem}.md has no owning ticket"));
                    }
                }
            }
        }
    }
    audit.orphans.sort();
    audit.dangling.sort();
    Ok(audit)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use gaugeblock_contracts::storypole::TaskPlan;
    use storypole_core::model::LedgerEvent;

    use crate::handlers::remint::test_support::task;

    /// A ticket whose plan names `pointer`, or none when it is empty.
    fn with_plan(key: &str, pointer: Option<&str>) -> TicketRecord {
        let mut created = task(key);
        created.plan = pointer.map(|path| TaskPlan {
            summary: "a plan".to_string(),
            design_markdown: String::new(),
            steps: Vec::new(),
            plan_file_path: Some(path.to_string()),
            updated_at_epoch_ms: 1,
        });
        TicketRecord::new(
            created.clone(),
            vec![LedgerEvent::TaskCreated { task: created }],
        )
    }

    fn write(root: &Path, record: &TicketRecord) {
        let dir = root.join(".gaugeblock/storypole/tickets");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{}.json", record.ticket.ticket_key)),
            serde_json::to_string(record).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn an_orphan_blueprint_and_a_dangling_pointer_are_both_reported() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(
            root,
            &with_plan("SP-1", Some(".gaugeblock/storypole/plans/SP-1.md")),
        );
        let plans = root.join(".gaugeblock/storypole/plans");
        std::fs::create_dir_all(&plans).unwrap();
        std::fs::write(plans.join("SP-9.md"), "# a blueprint nobody owns").unwrap();

        let audit = audit(root).unwrap();
        assert_eq!(audit.orphans.len(), 1, "the orphan is named");
        assert!(audit.orphans[0].contains("SP-9"));
        assert_eq!(audit.dangling.len(), 1, "the missing target is named");
        assert!(audit.dangling[0].contains("SP-1"));
        assert!(!audit.is_clean());
    }

    #[test]
    fn a_plan_file_that_exists_is_clean() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(
            root,
            &with_plan("SP-1", Some(".gaugeblock/storypole/plans/SP-1.md")),
        );
        let plans = root.join(".gaugeblock/storypole/plans");
        std::fs::create_dir_all(&plans).unwrap();
        std::fs::write(plans.join("SP-1.md"), "# the plan").unwrap();

        assert!(audit(root).unwrap().is_clean());
    }
}
