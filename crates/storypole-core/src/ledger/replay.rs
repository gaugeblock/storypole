//! Deterministic event replay engine that materializes tasks from `LedgerEvent` streams.

use crate::model::LedgerEvent;
use crate::model::decision::DecisionStatus;
use crate::model::task::{TaskKind, TaskNode, TaskStatus};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Replay a sequence of events to rebuild the active in-memory task map.
pub fn replay_events(events: &[LedgerEvent]) -> BTreeMap<Uuid, TaskNode> {
    let mut tasks = BTreeMap::new();

    for event in events {
        match event {
            LedgerEvent::TaskCreated { task } => {
                tasks.insert(task.id, task.clone());
            }
            LedgerEvent::DeferredItemCaptured { task, .. } => {
                tasks.insert(task.id, task.clone());
            }
            LedgerEvent::TaskUpdated {
                task_id,
                title,
                description,
                priority,
                scope,
                acceptance_criteria,
                timestamp_ms,
            } => {
                if let Some(task) = tasks.get_mut(task_id) {
                    if let Some(t) = title {
                        task.title = t.clone();
                    }
                    if let Some(d) = description {
                        task.description = d.clone();
                    }
                    if let Some(p) = priority {
                        task.priority = *p;
                    }
                    if let Some(s) = scope {
                        task.scope = s.clone();
                    }
                    if let Some(ac) = acceptance_criteria {
                        task.acceptance_criteria = ac.clone();
                    }
                    task.updated_at_epoch_ms = *timestamp_ms;
                }
            }
            LedgerEvent::StatusChanged {
                task_id,
                to,
                timestamp_ms,
                ..
            } => {
                if let Some(task) = tasks.get_mut(task_id) {
                    task.status = *to;
                    task.updated_at_epoch_ms = *timestamp_ms;
                }
            }
            LedgerEvent::EdgeAdded {
                task_id,
                edge,
                timestamp_ms,
            } => {
                if let Some(task) = tasks.get_mut(task_id) {
                    if !task.edges.contains(edge) {
                        task.edges.push(edge.clone());
                    }
                    task.updated_at_epoch_ms = *timestamp_ms;
                }
            }
            LedgerEvent::EdgeRemoved {
                task_id,
                edge,
                timestamp_ms,
            } => {
                if let Some(task) = tasks.get_mut(task_id) {
                    task.edges.retain(|e| e != edge);
                    task.updated_at_epoch_ms = *timestamp_ms;
                }
            }
            LedgerEvent::DecisionRaised {
                task_id,
                decision,
                timestamp_ms,
            } => {
                if let Some(task) = tasks.get_mut(task_id) {
                    if let Some(existing) = task
                        .decisions
                        .iter_mut()
                        .find(|d| d.decision_id == decision.decision_id)
                    {
                        *existing = decision.clone();
                    } else {
                        task.decisions.push(decision.clone());
                    }
                    task.updated_at_epoch_ms = *timestamp_ms;
                }
            }
            LedgerEvent::DecisionResolved {
                task_id,
                decision_id,
                chosen_option,
                rationale,
                resolved_by,
                spawned_followup,
                appended_criterion,
                timestamp_ms,
            } => {
                if let Some(task) = tasks.get_mut(task_id) {
                    if let Some(decision) = task
                        .decisions
                        .iter_mut()
                        .find(|d| d.decision_id == *decision_id)
                    {
                        decision.status = DecisionStatus::Resolved;
                        decision.chosen_option = Some(chosen_option.clone());
                        decision.rationale = Some(rationale.clone());
                        decision.resolved_by = Some(resolved_by.clone());
                        decision.updated_at_epoch_ms = *timestamp_ms;
                    }
                    if let Some(followup) = spawned_followup {
                        task.decisions.push(followup.clone());
                    }
                    if let Some(crit) = appended_criterion {
                        task.acceptance_criteria.push(crit.clone());
                    }
                    task.updated_at_epoch_ms = *timestamp_ms;
                }
            }
            LedgerEvent::DecisionSuperseded {
                task_id,
                decision_id,
                reason,
                timestamp_ms,
            } => {
                if let Some(task) = tasks.get_mut(task_id) {
                    cascade_supersede(task, *decision_id, reason, *timestamp_ms);
                    task.updated_at_epoch_ms = *timestamp_ms;
                }
            }
            LedgerEvent::TaskClaimed { task_id, claim } => {
                if let Some(task) = tasks.get_mut(task_id) {
                    task.active_claim = Some(claim.clone());
                    task.updated_at_epoch_ms = claim.claimed_at_epoch_ms;
                }
            }
            LedgerEvent::TaskReleased {
                task_id,
                timestamp_ms,
                ..
            } => {
                if let Some(task) = tasks.get_mut(task_id) {
                    task.active_claim = None;
                    task.updated_at_epoch_ms = *timestamp_ms;
                }
            }
            LedgerEvent::HandoffRecorded {
                task_id,
                handoff,
                new_status,
            } => {
                if let Some(task) = tasks.get_mut(task_id) {
                    task.latest_handoff = Some(handoff.clone());
                    task.status = *new_status;
                    task.active_claim = None;
                    task.updated_at_epoch_ms = handoff.timestamp_ms;
                }
            }
            LedgerEvent::TaskSplit {
                parent_task_id,
                spawned_children,
                timestamp_ms,
            } => {
                if let Some(parent) = tasks.get_mut(parent_task_id) {
                    parent.kind = TaskKind::Epic;
                    parent.updated_at_epoch_ms = *timestamp_ms;
                }
                for child in spawned_children {
                    tasks.insert(child.id, child.clone());
                }
            }
            LedgerEvent::TaskVerified { task_id, receipt } => {
                if let Some(task) = tasks.get_mut(task_id) {
                    task.status = TaskStatus::Verified;
                    task.verification_receipt = Some(receipt.clone());
                    task.active_claim = None;
                    task.updated_at_epoch_ms = receipt.verified_at_epoch_ms;
                }
            }
        }
    }

    tasks
}

fn cascade_supersede(task: &mut TaskNode, decision_id: Uuid, reason: &str, timestamp_ms: u64) {
    let mut to_supersede = vec![decision_id];
    let mut idx = 0;
    while idx < to_supersede.len() {
        let current = to_supersede[idx];
        idx += 1;
        for d in &task.decisions {
            if d.parent_decision_id == Some(current) && !to_supersede.contains(&d.decision_id) {
                to_supersede.push(d.decision_id);
            }
        }
    }

    for d in &mut task.decisions {
        if to_supersede.contains(&d.decision_id) {
            d.status = DecisionStatus::Superseded;
            d.updated_at_epoch_ms = timestamp_ms;
            let note = format!(" [Superseded: {reason}]");
            if let Some(ref mut r) = d.rationale {
                r.push_str(&note);
            } else {
                d.rationale = Some(note);
            }
        }
    }
}
