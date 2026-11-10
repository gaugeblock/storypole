//! Whole-token rewriting of keys and the text that cites them (`GB-347`).
//!
//! A key is not only an identifier: it is how one ticket points at another. A remint that moves keys without moving
//! what cites them leaves a live graph pointing at nothing, so every rewrite goes through here and every one is
//! counted, because a count that is reported is a count that can be argued with.

use std::collections::BTreeMap;

use serde_json::Value;
use storypole_core::ticket_keys::Remint;

// Re-exported so the passes and their tests keep one import path for the rules they rewrite under.
pub(crate) use super::rules::{
    PlanPath, Rewrite, cites, cites_bare, rewrite_prose, rewrite_prose_list,
};

/// Points one edge at the reminted key, when it names one that moved.
pub(crate) fn rewrite_edge_target(edge: &mut Value, lookup: &BTreeMap<String, String>) -> usize {
    let Some(object) = edge.as_object_mut() else {
        return 0;
    };
    let Some(target) = object
        .get("target_ticket_key")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return 0;
    };
    let Some(new_key) = lookup.get(&target) else {
        return 0;
    };
    object.insert(
        "target_ticket_key".to_string(),
        Value::String(new_key.clone()),
    );
    1
}

/// Rewrites the referencing surface of one task-shaped object
///
/// Everything replay serves as current state, in one place: edge targets, the title and description, acceptance
/// criteria, attached decisions, the verification receipt, the latest handoff and the attached plan. Used for the
/// projection and for every task-shaped event payload - `task_created`, `deferred_item_captured` and each split child -
/// because they are the same shape and all of them are read.
///
/// The title earned its place here the hard way: `benchwork`'s `BW-27` cites a sibling in its title, the replay check
/// refused the store, and the remint had to be restored and re-run (`GB-347`).
pub(crate) fn rewrite_task(
    task: &mut Value,
    lookup: &BTreeMap<String, String>,
    mapping: &[Remint],
    rules: Rewrite,
) -> (usize, usize) {
    let edges = match task.get_mut("edges").and_then(Value::as_array_mut) {
        Some(edges) => edges
            .iter_mut()
            .map(|edge| rewrite_edge_target(edge, lookup))
            .sum::<usize>(),
        None => 0,
    };

    let mut citations = rewrite_prose(task, "title", mapping, rules);
    citations += rewrite_prose(task, "description", mapping, rules);
    citations += rewrite_prose_list(task, "acceptance_criteria", mapping, rules);

    if let Some(decisions) = task.get_mut("decisions").and_then(Value::as_array_mut) {
        for decision in decisions {
            citations += rewrite_decision(decision, mapping, rules);
        }
    }
    if let Some(receipt) = task.get_mut("verification_receipt") {
        citations += rewrite_prose(receipt, "summary", mapping, rules);
        citations += rewrite_prose(receipt, "command", mapping, rules);
    }
    if let Some(handoff) = task.get_mut("latest_handoff") {
        citations += rewrite_handoff(handoff, mapping, rules);
    }
    if let Some(plan) = task.get_mut("plan") {
        citations += rewrite_plan(plan, mapping, rules);
    }
    (edges, citations)
}

/// Rewrites an attached plan: its prose, its summary and the path it is stored at.
///
/// `design_markdown` is the plan's text stored inline rather than a pointer to it, and it is served as current state,
/// so it is rewritten under the same rule as a description (`GB-347` decision, 2026-11-10). `plan_file_path` moves with
/// the key because the file is named for the ticket.
pub(crate) fn rewrite_plan(plan: &mut Value, mapping: &[Remint], rules: Rewrite) -> usize {
    let mut citations = rewrite_prose(plan, "design_markdown", mapping, rules);
    citations += rewrite_prose(plan, "summary", mapping, rules);
    if rules.plan_path == PlanPath::Moves {
        citations += rewrite_prose(plan, "plan_file_path", mapping, rules);
    }
    if let Some(steps) = plan.get_mut("steps").and_then(Value::as_array_mut) {
        for step in steps {
            citations += rewrite_prose(step, "title", mapping, rules);
            citations += rewrite_prose(step, "verification_gate", mapping, rules);
            citations += rewrite_prose_list(step, "target_files", mapping, rules);
        }
    }
    citations
}

/// Rewrites the prose a decision carries: the question, its options and the answer's justification.
pub(crate) fn rewrite_decision(decision: &mut Value, mapping: &[Remint], rules: Rewrite) -> usize {
    let mut citations = rewrite_prose(decision, "question", mapping, rules);
    citations += rewrite_prose_list(decision, "proposed_options", mapping, rules);
    citations += rewrite_prose(decision, "chosen_option", mapping, rules);
    citations += rewrite_prose(decision, "rationale", mapping, rules);
    citations
}

/// Rewrites the prose a handoff carries.
pub(crate) fn rewrite_handoff(handoff: &mut Value, mapping: &[Remint], rules: Rewrite) -> usize {
    let mut citations = rewrite_prose_list(handoff, "completed_items", mapping, rules);
    citations += rewrite_prose_list(handoff, "remaining_items", mapping, rules);
    citations += rewrite_prose(handoff, "last_error_or_blocker", mapping, rules);
    citations
}
