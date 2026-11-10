//! Rewriting the event payloads that replay actually reads (`GB-347`).
//!
//! A ticket file is a projection plus a log, and the log is the source of truth: replay applies
//! `TaskUpdated.description`, `EdgeAdded.edge` and `PlanUpdated.plan` **from the event**, not from the snapshot beside
//! it. Rewriting only the snapshots therefore left the served state citing keys that no longer existed - and it passed
//! verification, because the key field itself was right. `batten` could not show this: one ticket, no plan, no edge
//! event, no event-level description. `common` shows it in 12 of 14 tickets.
//!
//! One field is deliberately left alone. A claim's `branch` records where work happened, and renaming it would make the
//! record name a branch that never existed; history stays history, even when it reads as stale.

use std::collections::BTreeMap;

use serde_json::Value;
use storypole_core::ticket_keys::Remint;

use super::rewrite::{
    Rewrite, rewrite_decision, rewrite_edge_target, rewrite_handoff, rewrite_plan, rewrite_prose,
    rewrite_prose_list, rewrite_task,
};

/// Rewrites every live reference inside one event, returning `(edge targets, citations)` rewritten.
pub(crate) fn rewrite_event(
    event: &mut Value,
    lookup: &BTreeMap<String, String>,
    mapping: &[Remint],
    rules: Rewrite,
) -> (usize, usize) {
    let (mut edges, mut citations) = (0, 0);
    let tag = event
        .get("event")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    match tag.as_str() {
        // The whole task travels in the payload, so its own key moves with it.
        "task_created" | "deferred_item_captured" => {
            if let Some(task) = event.get_mut("task") {
                let (e, c) = remap_task(task, lookup, mapping, rules);
                edges += e;
                citations += c;
            }
        }
        "task_split" => {
            if let Some(children) = event
                .get_mut("spawned_children")
                .and_then(Value::as_array_mut)
            {
                for child in children {
                    let (e, c) = remap_task(child, lookup, mapping, rules);
                    edges += e;
                    citations += c;
                }
            }
        }
        // Replay overwrites the task's description, title and criteria from here.
        "task_updated" => {
            citations += rewrite_prose(event, "description", mapping, rules);
            citations += rewrite_prose(event, "title", mapping, rules);
            citations += rewrite_prose_list(event, "acceptance_criteria", mapping, rules);
        }
        // Replay pushes the edge straight into the task's edge list.
        "edge_added" | "edge_removed" => {
            if let Some(edge) = event.get_mut("edge") {
                edges += rewrite_edge_target(edge, lookup);
            }
        }
        "decision_raised" => {
            if let Some(decision) = event.get_mut("decision") {
                citations += rewrite_decision(decision, mapping, rules);
            }
        }
        "decision_resolved" => {
            citations += rewrite_prose(event, "chosen_option", mapping, rules);
            citations += rewrite_prose(event, "rationale", mapping, rules);
            citations += rewrite_prose(event, "appended_criterion", mapping, rules);
            if let Some(followup) = event.get_mut("spawned_followup") {
                citations += rewrite_decision(followup, mapping, rules);
            }
        }
        "decision_superseded" | "status_changed" => {
            citations += rewrite_prose(event, "reason", mapping, rules);
        }
        "handoff_recorded" => {
            if let Some(handoff) = event.get_mut("handoff") {
                citations += rewrite_handoff(handoff, mapping, rules);
            }
        }
        // Replay installs this plan wholesale, so the attachment is the served state.
        "plan_updated" => {
            if let Some(plan) = event.get_mut("plan") {
                citations += rewrite_plan(plan, mapping, rules);
            }
        }
        "task_verified" => {
            if let Some(receipt) = event.get_mut("receipt") {
                citations += rewrite_prose(receipt, "summary", mapping, rules);
                citations += rewrite_prose(receipt, "command", mapping, rules);
            }
        }
        // Attachment, release, archive and unarchive carry no prose that replay serves. `task_claimed` carries
        // `claim.branch`, which is history by decision and is left untouched on purpose.
        _ => {}
    }
    (edges, citations)
}

/// Moves a task snapshot's own key, then rewrites what it references.
fn remap_task(
    task: &mut Value,
    lookup: &BTreeMap<String, String>,
    mapping: &[Remint],
    rules: Rewrite,
) -> (usize, usize) {
    let old_key = task
        .get("ticket_key")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(old_key) = old_key
        && let Some(new_key) = lookup.get(&old_key)
        && let Some(object) = task.as_object_mut()
    {
        object.insert("ticket_key".to_string(), Value::String(new_key.clone()));
    }
    rewrite_task(task, lookup, mapping, rules)
}
