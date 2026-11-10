//! What the remint rewrites, field by field (`GB-347`).
//!
//! Every case here is one the live run either hit or would have hit: an event payload replay installs, an edge inside
//! such a payload, a plan that stores its text inline, a creation snapshot's title, and a claim's branch that must not
//! move. The replay check itself is proved in `verify_tests`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::{Value, json};
use uuid::Uuid;

use super::events::rewrite_event;
use super::rewrite::{rewrite_plan, rewrite_task};
use super::test_support::{lookup, mapping};

#[test]
fn a_task_update_rewrites_the_text_replay_installs() {
    let mut event = json!({
        "event": "task_updated",
        "task_id": Uuid::new_v4(),
        "title": "blocked by GB-182",
        "description": "GB-182 holds the detail, GB-1821 does not",
        "priority": null,
        "scope": null,
        "acceptance_criteria": ["GB-182 lands"],
        "timestamp_ms": 5
    });
    let (edges, citations) = rewrite_event(&mut event, &lookup(), &mapping());
    assert_eq!((edges, citations), (0, 3));
    assert_eq!(event["title"], json!("blocked by SP-7"));
    assert_eq!(
        event["description"],
        json!("SP-7 holds the detail, GB-1821 does not")
    );
    assert_eq!(event["acceptance_criteria"], json!(["SP-7 lands"]));
}

#[test]
fn an_edge_event_moves_its_target_and_leaves_others_alone() {
    let mut event = json!({
        "event": "edge_added",
        "task_id": Uuid::new_v4(),
        "edge": {"kind": "parent_epic", "target_id": Uuid::new_v4(), "target_ticket_key": "GB-182"},
        "timestamp_ms": 5
    });
    assert_eq!(rewrite_event(&mut event, &lookup(), &mapping()).0, 1);
    assert_eq!(event["edge"]["target_ticket_key"], json!("SP-7"));

    let mut other = json!({
        "event": "edge_added",
        "edge": {"kind": "related_to", "target_ticket_key": "GB-999"}
    });
    assert_eq!(rewrite_event(&mut other, &lookup(), &mapping()).0, 0);
    assert_eq!(other["edge"]["target_ticket_key"], json!("GB-999"));
}

#[test]
fn a_plan_moves_its_path_and_the_markdown_it_stores_inline() {
    let mut plan = json!({
        "summary": "carries GB-182",
        "design_markdown": "# Plan: GB-182\n\nSee `GB-182.md`.",
        "plan_file_path": ".storypole/plans/GB-182.md",
        "steps": [{"step_id": "1", "title": "do GB-182", "target_files": ["GB-182.rs"],
                   "verification_gate": "GB-182 passes", "completed": false}],
        "updated_at_epoch_ms": 5
    });
    assert_eq!(rewrite_plan(&mut plan, &mapping()), 6);
    assert_eq!(plan["plan_file_path"], json!(".storypole/plans/SP-7.md"));
    assert!(
        plan["design_markdown"]
            .as_str()
            .unwrap()
            .contains("SP-7.md")
    );
    assert_eq!(plan["steps"][0]["target_files"], json!(["SP-7.rs"]));
}

#[test]
fn a_claim_branch_is_history_and_stays_as_it_was() {
    let mut event = json!({
        "event": "task_claimed",
        "task_id": Uuid::new_v4(),
        "claim": {"agent_id": "an-agent", "branch": "feat/GB-182-remint", "base_commit_sha": "abc"}
    });
    assert_eq!(rewrite_event(&mut event, &lookup(), &mapping()), (0, 0));
    assert_eq!(event["claim"]["branch"], json!("feat/GB-182-remint"));
}

#[test]
fn a_creation_snapshot_rewrites_its_title_as_well_as_its_description() {
    // benchwork's BW-27 cites a sibling in its title, and the creation snapshot is what replay installs for it. The
    // replay check refused that store after a remint that rewrote the description and missed the title (GB-347).
    let mut snapshot = json!({
        "event": "task_created",
        "task": {
            "ticket_key": "GB-27",
            "title": "depends on GB-182",
            "description": "GB-182 again",
            "edges": [{"kind": "related_to", "target_ticket_key": "GB-182"}]
        }
    });
    let (edges, citations) = rewrite_event(&mut snapshot, &lookup(), &mapping());
    assert_eq!((edges, citations), (1, 2));
    assert_eq!(snapshot["task"]["title"], json!("depends on SP-7"));
    assert_eq!(snapshot["task"]["description"], json!("SP-7 again"));
    assert_eq!(
        snapshot["task"]["edges"][0]["target_ticket_key"],
        json!("SP-7")
    );
}

#[test]
fn a_value_that_is_not_a_ticket_is_left_alone() {
    let mut scalar = Value::Number(1.into());
    assert_eq!(rewrite_event(&mut scalar, &lookup(), &mapping()), (0, 0));
    assert_eq!(rewrite_task(&mut scalar, &lookup(), &mapping()), (0, 0));
}
