//! The cross-store pass, end to end on a synthetic suite (`GB-347`).
//!
//! Each test builds two stores the way a real suite is laid out: a harness store at the root, a member beside it, and a
//! recorded remint in the member. No git, no network, no clock dependency beyond the timestamp the edge carries.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use storypole_core::model::LedgerEvent;
use storypole_core::tickets::TicketRecord;

use crate::handlers::remint::record;
use crate::handlers::remint::test_support::task;

use super::apply;
use super::build;

/// A record whose description cites `cited`, which is how a harness ticket names the work.
fn described(key: &str, cited: &str) -> TicketRecord {
    let mut created = task(key);
    created.description = format!("the work is carried by {cited}");
    TicketRecord::new(
        created.clone(),
        vec![LedgerEvent::TaskCreated { task: created }],
    )
}

/// Writes one record into `dir`.
pub(super) fn write_record(dir: &Path, record: &TicketRecord) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join(format!("{}.json", record.ticket.ticket_key)),
        serde_json::to_string(record).unwrap(),
    )
    .unwrap();
}

/// A suite with one harness ticket and one member whose ticket was reminted from `from` to `to`.
pub(super) fn suite(
    harness: &str,
    cited: &str,
    from: &str,
    to: &str,
) -> (TempDir, PathBuf, TicketRecord) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let member_ticket = task(to);
    write_record(
        &member_store(&root),
        &TicketRecord::new(member_ticket.clone(), Vec::new()),
    );
    let member_record = described(cited, "nothing");
    record::append(
        &root.join("member"),
        &[record::AppliedRemint {
            id: member_ticket.id,
            from: from.to_string(),
            to: to.to_string(),
        }],
    )
    .unwrap();
    let harness_record = described(harness, cited);
    write_record(&root_store(&root), &harness_record);
    let _ = member_record;
    (dir, root, harness_record)
}

/// Where the harness keeps its tickets.
pub(super) fn root_store(root: &Path) -> PathBuf {
    root.join(".gaugeblock/storypole/tickets")
}

/// Where the member keeps its tickets.
pub(super) fn member_store(root: &Path) -> PathBuf {
    root.join("member/.gaugeblock/storypole/tickets")
}

#[test]
fn a_dead_key_in_a_harness_ticket_is_repaired_and_linked() {
    let (_dir, root, harness) = suite("GB-1", "GB-9", "GB-9", "MTR-1");

    let planned = build::build(&root).unwrap();
    assert_eq!(planned.moved.len(), 1);
    assert_eq!(planned.repairs.len(), 1);
    assert_eq!(planned.links.len(), 1);
    assert_eq!(planned.links[0].harness, "GB-1");
    assert_eq!(planned.links[0].target.store, "member");

    let applied = apply::apply(&root, &planned).unwrap();
    assert_eq!((applied.tickets, applied.links), (1, 1));

    let after = build::build(&root).unwrap();
    assert!(
        after.repairs.is_empty(),
        "a repaired citation is not cited again"
    );
    assert!(
        after.links.is_empty(),
        "the link exists, so it is not proposed again"
    );

    // The served text carries the new key and an edge by uuid, which survives any later rename.
    let text = std::fs::read_to_string(
        root_store(&root).join(format!("{}.json", harness.ticket.ticket_key)),
    )
    .unwrap();
    let record: TicketRecord = serde_json::from_str(&text).unwrap();
    let replayed = record.replay().unwrap();
    assert!(replayed.description.contains("MTR-1"));
    assert!(!replayed.description.contains("GB-9"));
    assert_eq!(replayed.edges.len(), 1);
    assert_eq!(
        replayed.edges[0].target_ticket_key.as_deref(),
        Some("MTR-1")
    );
    assert_eq!(replayed.edges[0].target_repo.as_deref(), Some("member"));
}

#[test]
fn a_key_the_harness_still_uses_is_not_dead_and_is_left_alone() {
    // The collision case: the same key names a live harness ticket, so a citation of it means the harness.
    let (_dir, root, _harness) = suite("GB-1", "GB-9", "GB-9", "MTR-1");
    write_record(&root_store(&root), &described("GB-9", "nothing"));

    let planned = build::build(&root).unwrap();
    assert!(
        planned.moved.is_empty(),
        "a key that is still owned is not dead"
    );
    assert!(planned.repairs.is_empty());
    assert!(planned.links.is_empty());
}

#[test]
fn a_key_two_members_claim_is_reported_rather_than_guessed() {
    let (_dir, root, _harness) = suite("GB-1", "GB-9", "GB-9", "MTR-1");
    record::append(
        &root.join("other"),
        &[record::AppliedRemint {
            id: task("OF-1").id,
            from: "GB-9".to_string(),
            to: "OF-1".to_string(),
        }],
    )
    .unwrap();
    std::fs::create_dir_all(root.join("other/.gaugeblock/storypole/tickets")).unwrap();

    let planned = build::build(&root).unwrap();
    assert_eq!(planned.conflicts.len(), 1);
    assert!(
        planned.moved.is_empty(),
        "an ambiguous key is not resolved by coin toss"
    );
    assert!(planned.repairs.is_empty());
}

#[test]
fn recording_a_remint_twice_adds_it_once() {
    let dir = tempfile::tempdir().unwrap();
    let entry = record::AppliedRemint {
        id: task("MTR-1").id,
        from: "GB-9".to_string(),
        to: "MTR-1".to_string(),
    };
    assert_eq!(
        record::append(dir.path(), std::slice::from_ref(&entry)).unwrap(),
        1
    );
    assert_eq!(
        record::append(dir.path(), std::slice::from_ref(&entry)).unwrap(),
        0
    );
    assert_eq!(record::load(dir.path()).unwrap().len(), 1);
}
