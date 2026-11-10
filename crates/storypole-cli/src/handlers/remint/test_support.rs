//! Fixtures the remint tests share (`GB-347`).
//!
//! Split out so the rewrite tests and the replay tests each stay small enough to read in one screen. They agree on one
//! mapping, `GB-182 -> SP-7`, so a failure in either module names the same two keys.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use uuid::Uuid;

use gaugeblock_contracts::storypole::{TaskKind, TaskScope, TaskStatus};
use storypole_core::model::{LedgerEvent, TaskNode};
use storypole_core::ticket_keys::Remint;
use storypole_core::tickets::TicketRecord;

/// The mapping every test moves: one key, `GB-182 -> SP-7`.
pub(super) fn mapping() -> Vec<Remint> {
    vec![Remint {
        from: "GB-182".to_string(),
        to: "SP-7".to_string(),
    }]
}

/// The same mapping as a lookup, which is how edge targets resolve.
pub(super) fn lookup() -> BTreeMap<String, String> {
    BTreeMap::from([("GB-182".to_string(), "SP-7".to_string())])
}

/// A task with the fields a record actually reads.
pub(super) fn task(key: &str) -> TaskNode {
    TaskNode {
        id: Uuid::new_v4(),
        ticket_key: key.to_string(),
        slug: format!("{key}-slug"),
        title: "a ticket".to_string(),
        description: String::new(),
        kind: TaskKind::Feature,
        status: TaskStatus::Ready,
        priority: 3,
        scope: TaskScope {
            repo: Some("storypole".to_string()),
            crate_name: None,
            module_path: None,
            target_files: Vec::new(),
            target_symbols: Vec::new(),
            labels: Vec::new(),
        },
        edges: Vec::new(),
        decisions: Vec::new(),
        acceptance_criteria: Vec::new(),
        plan: None,
        active_claim: None,
        latest_handoff: None,
        verification_receipt: None,
        created_at_epoch_ms: 1,
        updated_at_epoch_ms: 1,
        archived_at_epoch_ms: None,
    }
}

/// A task update that cites `cited`, the field replay takes the description from.
pub(super) fn update_citing(id: Uuid, cited: &str) -> LedgerEvent {
    LedgerEvent::TaskUpdated {
        task_id: id,
        title: None,
        description: Some(format!("see {cited}")),
        priority: None,
        scope: None,
        acceptance_criteria: None,
        timestamp_ms: 9,
    }
}

/// A record that was created and then had its description set, which is how a ticket comes to cite a key.
pub(super) fn record_described_with(key: &str, cited: &str) -> TicketRecord {
    let created = task(key);
    let id = created.id;
    TicketRecord::new(
        created.clone(),
        vec![
            LedgerEvent::TaskCreated { task: created },
            update_citing(id, cited),
        ],
    )
}
