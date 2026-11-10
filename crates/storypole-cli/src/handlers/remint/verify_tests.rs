//! The replay check that decides whether a remint may be trusted (`GB-347`).
//!
//! These two tests are the reason the verb changed: the file a remint writes is not the state the tool serves, so a
//! check that reads the file back can pass while the store is broken. One test proves a rewritten record replays to the
//! new key; the other proves a record that still cites the old one is refused rather than certified.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::json;
use tempfile::tempdir;

use super::events::rewrite_event;
use super::rewrite::rewrite_task;
use super::test_support::{lookup, mapping, record_described_with};
use super::verify::verify_replay;

#[test]
fn a_rewritten_record_replays_to_the_new_key() {
    let mut record = record_described_with("GB-182", "GB-182");
    let mut document = serde_json::to_value(&record).unwrap();
    if let Some(ticket) = document.get_mut("ticket") {
        ticket["ticket_key"] = json!("SP-7");
        rewrite_task(ticket, &lookup(), &mapping());
    }
    for event in document["events"].as_array_mut().unwrap() {
        rewrite_event(event, &lookup(), &mapping());
    }
    record = serde_json::from_value(document).unwrap();

    let replayed = record.replay().unwrap();
    assert_eq!(replayed.ticket_key, "SP-7");
    assert_eq!(replayed.description, "see SP-7");
}

#[test]
fn a_ticket_that_still_cites_the_old_key_is_refused() {
    let dir = tempdir().unwrap();
    let tickets = dir.path().join(".gaugeblock/storypole/tickets");
    std::fs::create_dir_all(&tickets).unwrap();

    let stale = record_described_with("SP-7", "GB-182");
    std::fs::write(
        tickets.join("SP-7.json"),
        serde_json::to_string(&stale).unwrap(),
    )
    .unwrap();

    let failure = verify_replay(dir.path(), &mapping())
        .unwrap_err()
        .to_string();
    assert!(
        failure.contains("GB-182"),
        "message should name the stale key: {failure}"
    );
    assert!(
        failure.contains("description"),
        "message should name the field: {failure}"
    );

    // The same record with its served text already moved is accepted, so the check can pass as well as fail.
    let moved = record_described_with("SP-7", "SP-7");
    std::fs::write(
        tickets.join("SP-7.json"),
        serde_json::to_string(&moved).unwrap(),
    )
    .unwrap();
    assert!(verify_replay(dir.path(), &mapping()).is_ok());
}
