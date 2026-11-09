//! The workspace rollup reads whichever layout each member uses (`GB-330`).
//!
//! Two properties, both earned the hard way: a member in records mode must contribute to the rollup, and one
//! task id in two places must stop the rollup rather than be merged. `GB-79` is what merging looks like - two
//! copies of a ticket that disagreed about whether the work was done.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use gaugeblock_contracts::storypole::{TaskKind, TaskScope, TaskStatus};
use gaugeblock_contracts::{LedgerEvent, TaskNode};
use storypole_core::config::StorypoleConfig;
use storypole_core::store::StorypoleStore;
use storypole_core::tickets::{self, TicketRecord};
use storypole_core::workspace::store::StorypoleWorkspaceStore;
use tempfile::TempDir;
use uuid::Uuid;

fn task(id: Uuid, key: &str) -> TaskNode {
    TaskNode {
        id,
        ticket_key: key.to_string(),
        slug: format!("{key}-slug"),
        title: format!("the {key} ticket"),
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

/// A member in records mode holding one ticket.
fn records_member(root: &std::path::Path, name: &str, id: Uuid, key: &str) {
    let path = root.join(name);
    fs::create_dir_all(&path).unwrap();
    StorypoleStore::init(&path, StorypoleConfig::default()).expect("init member");
    let dot = path.join(".gaugeblock").join("storypole");
    fs::create_dir_all(tickets::dir(&dot)).unwrap();
    let record = TicketRecord::new(
        task(id, key),
        vec![LedgerEvent::TaskCreated {
            task: task(id, key),
        }],
    );
    tickets::write(&tickets::path(&dot, key).unwrap(), &record).unwrap();
}

#[test]
fn a_member_in_records_mode_contributes_to_the_rollup() {
    let temp = TempDir::new().unwrap();
    records_member(temp.path(), "alpha", Uuid::from_u128(1), "SP-1");
    records_member(temp.path(), "beta", Uuid::from_u128(2), "SP-2");

    let workspace = StorypoleWorkspaceStore::open(temp.path()).expect("rollup");
    let mut stmt = workspace
        .conn()
        .prepare("SELECT ticket_key FROM tasks ORDER BY ticket_key")
        .unwrap();
    let keys: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .filter_map(Result::ok)
        .collect();

    assert_eq!(keys, vec!["SP-1", "SP-2"], "both members are visible");
}

#[test]
fn one_task_in_two_members_is_refused_rather_than_merged() {
    // The GB-79 shape: the same task id in two stores. There is no correct merge, so the rollup names both
    // places and stops; picking a winner is how two copies came to disagree about whether work was done.
    let temp = TempDir::new().unwrap();
    let shared = Uuid::from_u128(7);
    records_member(temp.path(), "alpha", shared, "SP-1");
    records_member(temp.path(), "beta", shared, "SP-1");

    let error = StorypoleWorkspaceStore::open(temp.path()).expect_err("duplicate task id");
    let message = error.to_string();
    assert!(message.contains("both alpha and beta"), "{message}");
    assert!(
        message.contains("one ticket belongs in one file"),
        "{message}"
    );
}
