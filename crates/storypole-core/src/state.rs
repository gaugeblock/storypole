//! Reading a store's contents without knowing which layout it uses (`GB-348`).
//!
//! Consumers kept deciding which file to read, and one of them spelled `ledger.jsonl` directly - so when
//! tickets became one file each, that reader silently returned nothing and the feature it fed went blank. The
//! layout is storypole's business, so this module answers the question a consumer actually has: what tasks and
//! events does this store hold?
//!
//! Nothing here writes. Writes go through [`crate::store::StorypoleStore`], which routes by layout; a consumer
//! that needs to change state should be calling the ops layer rather than editing files either.

use std::collections::BTreeMap;
use std::path::Path;

use uuid::Uuid;

use crate::error::StorypoleError;
use crate::ledger::read_events;
use crate::ledger::replay::replay_events;
use crate::model::{LedgerEvent, TaskNode};
use crate::storage_mode::StorageMode;

/// Everything a store holds, and which layout it was stored in.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedState {
    /// The layout the contents came from, for a caller that needs to report or diagnose it.
    pub layout: StorageMode,
    /// Every event, in the order the store keeps them. Record events arrive per ticket, so a caller that needs
    /// one global order sorts by timestamp; replay does not care.
    pub events: Vec<LedgerEvent>,
    /// Tasks by id, as of the last event.
    pub tasks: BTreeMap<Uuid, TaskNode>,
}

/// Reads every task and event a store holds.
///
/// An absent store is empty rather than an error: a directory that has no `.gaugeblock/storypole` has no
/// tickets, and a consumer rendering a panel should show nothing rather than fail.
///
/// # Errors
///
/// [`StorypoleError::TicketRecord`] for an unreadable or corrupt record, [`StorypoleError::CorruptLedger`] for a
/// malformed ledger line.
pub fn load(dot_dir: &Path) -> Result<LoadedState, StorypoleError> {
    let layout = StorageMode::detect(dot_dir);
    let events = match layout {
        StorageMode::Records => {
            let mut collected = Vec::new();
            for record in crate::tickets::read_all(dot_dir)? {
                collected.extend(record.events);
            }
            collected
        }
        StorageMode::Ledger => read_events(&dot_dir.join("ledger.jsonl"))?,
    };
    let tasks = replay_events(&events);
    Ok(LoadedState {
        layout,
        events,
        tasks,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::fs;

    use gaugeblock_contracts::storypole::{TaskKind, TaskScope, TaskStatus};

    use super::*;
    use crate::tickets::{self, TicketRecord};

    fn task(id: Uuid, key: &str) -> TaskNode {
        TaskNode {
            id,
            ticket_key: key.to_string(),
            slug: format!("{key}-slug"),
            title: "a ticket".to_string(),
            description: String::new(),
            kind: TaskKind::Feature,
            status: TaskStatus::Ready,
            priority: 3,
            scope: TaskScope::default(),
            edges: Vec::new(),
            decisions: Vec::new(),
            acceptance_criteria: Vec::new(),
            plan: None,
            active_claim: None,
            latest_handoff: None,
            verification_receipt: None,
            created_at_epoch_ms: 10,
            updated_at_epoch_ms: 10,
            archived_at_epoch_ms: None,
        }
    }

    #[test]
    fn a_store_with_no_state_is_empty_rather_than_an_error() {
        let temp = tempfile::tempdir().unwrap();
        let loaded = load(temp.path()).unwrap();

        assert_eq!(loaded.layout, StorageMode::Ledger);
        assert!(loaded.tasks.is_empty());
    }

    #[test]
    fn records_are_read_from_ticket_files() {
        let temp = tempfile::tempdir().unwrap();
        let dot = temp.path().join(".gaugeblock").join("storypole");
        fs::create_dir_all(tickets::dir(&dot)).unwrap();
        let id = Uuid::from_u128(1);
        let record = TicketRecord::new(
            task(id, "SP-1"),
            vec![LedgerEvent::TaskCreated {
                task: task(id, "SP-1"),
            }],
        );
        tickets::write(&tickets::path(&dot, "SP-1").unwrap(), &record).unwrap();

        let loaded = load(&dot).unwrap();

        assert_eq!(loaded.layout, StorageMode::Records);
        assert_eq!(loaded.tasks.len(), 1, "the ticket is visible");
        assert_eq!(loaded.events.len(), 1);
    }

    #[test]
    fn a_legacy_ledger_is_still_read() {
        let temp = tempfile::tempdir().unwrap();
        let dot = temp.path().join(".gaugeblock").join("storypole");
        fs::create_dir_all(&dot).unwrap();
        let id = Uuid::from_u128(2);
        let line = serde_json::to_string(&LedgerEvent::TaskCreated {
            task: task(id, "SP-2"),
        })
        .unwrap();
        fs::write(dot.join("ledger.jsonl"), format!("{line}\n")).unwrap();

        let loaded = load(&dot).unwrap();

        assert_eq!(loaded.layout, StorageMode::Ledger);
        assert_eq!(loaded.tasks.len(), 1);
    }
}
