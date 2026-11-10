//! Which occurrences count as a citation, and which payload counts at all (`GB-351`).
//!
//! Split from the suite tests so each module stays readable: these are about the rules a relink rewrites under, and each
//! one failed before its rule existed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use storypole_core::model::LedgerEvent;
use storypole_core::tickets::TicketRecord;

use crate::handlers::remint::record;
use crate::handlers::remint::test_support::{task, update_citing};

use super::apply;
use super::build;
use super::tests::{member_store, root_store, suite, write_record};

#[test]
fn a_filename_is_not_a_citation() {
    // `GB-350`'s note quotes blueprints by filename, which produced three links to unrelated tickets before this rule:
    // a file named for a key is data, not a reference to the ticket that key used to name (GB-351).
    let (_dir, root, _harness) = suite("GB-1", "plans/GB-9.md", "GB-9", "MTR-1");

    let planned = build::build(&root).unwrap();
    assert!(planned.links.is_empty(), "a path is not a link");
    assert!(planned.repairs.is_empty(), "a path is not a citation");
}

#[test]
fn a_superseded_payload_is_left_alone() {
    // Replay keeps the last `task_updated` description, so rewriting an earlier one edits history for no effect on
    // state - and kept a corrected ticket reporting work to do (GB-351).
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    write_record(
        &member_store(&root),
        &TicketRecord::new(task("MTR-1"), Vec::new()),
    );
    record::append(
        &root.join("member"),
        &[record::AppliedRemint {
            id: task("MTR-1").id,
            from: "GB-9".to_string(),
            to: "MTR-1".to_string(),
        }],
    )
    .unwrap();

    // Two updates: the older cites the dead key, the newer - the one replay serves - does not.
    let created = task("GB-1");
    let id = created.id;
    let mut older = update_citing(id, "GB-9");
    if let LedgerEvent::TaskUpdated { timestamp_ms, .. } = &mut older {
        *timestamp_ms = 5;
    }
    let newer = update_citing(id, "an unrelated note");
    write_record(
        &root_store(&root),
        &TicketRecord::new(created, vec![older, newer]),
    );

    let planned = build::build(&root).unwrap();
    assert!(
        planned.repairs.is_empty(),
        "the citing payload is superseded, so the served text needs no repair"
    );
}

#[test]
fn a_path_is_left_alone_even_when_the_same_key_is_cited_bare() {
    // The residual GB-351 was filed with: a per-file filter cannot say which occurrence is which, so the rule has to live
    // in the replacement. Both shapes appear in this description and only the bare one may move.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    write_record(
        &member_store(&root),
        &TicketRecord::new(task("MTR-1"), Vec::new()),
    );
    record::append(
        &root.join("member"),
        &[record::AppliedRemint {
            id: task("MTR-1").id,
            from: "GB-9".to_string(),
            to: "MTR-1".to_string(),
        }],
    )
    .unwrap();

    let mut created = task("GB-1");
    created.description = "carried by GB-9, blueprint at plans/GB-9.md".to_string();
    write_record(
        &root_store(&root),
        &TicketRecord::new(
            created.clone(),
            vec![LedgerEvent::TaskCreated { task: created }],
        ),
    );

    let planned = build::build(&root).unwrap();
    assert_eq!(planned.repairs.len(), 1, "the bare citation is a repair");
    apply::apply(&root, &planned).unwrap();

    let text = std::fs::read_to_string(root_store(&root).join("GB-1.json")).unwrap();
    let record: TicketRecord = serde_json::from_str(&text).unwrap();
    let replayed = record.replay().unwrap();
    assert!(replayed.description.contains("carried by MTR-1"));
    assert!(
        replayed.description.contains("plans/GB-9.md"),
        "a filename is data, even beside a citation of the same key: {}",
        replayed.description
    );
}
