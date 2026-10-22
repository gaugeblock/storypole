//! Context briefing assembly, ready task selection, and file collision analysis.

use crate::config::StorypoleConfig;
use crate::model::decision::DecisionStatus;
use crate::model::events::TaskContextBriefing;
use crate::model::task::{EdgeKind, TaskNode, TaskStatus};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Construct a `TaskContextBriefing` with dependency summaries, decisions, and collision checks.
pub fn build_task_briefing(
    task: &TaskNode,
    all_tasks: &BTreeMap<Uuid, TaskNode>,
    config: &StorypoleConfig,
    current_epoch_ms: u64,
) -> TaskContextBriefing {
    let verification_command = task
        .scope
        .crate_name
        .as_deref()
        .and_then(|c| config.verification_command_for(c))
        .or_else(|| {
            task.scope
                .repo
                .as_deref()
                .and_then(|r| config.verification_command_for(r))
        })
        .map(|s| s.to_string());

    let mut related_tasks_summary = Vec::new();
    let mut inherited_decisions = Vec::new();

    for edge in &task.edges {
        if matches!(edge.kind, EdgeKind::RelatedTo | EdgeKind::BlockedBy)
            && let Some(target) = all_tasks.get(&edge.target_id)
        {
            related_tasks_summary.push((
                target.ticket_key.clone(),
                target.title.clone(),
                target.status,
            ));
            for d in &target.decisions {
                if d.status == DecisionStatus::Resolved {
                    inherited_decisions.push(d.clone());
                }
            }
        }
    }

    let mut file_collision_warnings = Vec::new();
    let mut known_deferred_issues_in_scope = Vec::new();

    for other in all_tasks.values() {
        if other.id == task.id {
            continue;
        }

        // Check file and symbol collisions with active InProgress tasks
        if other.status == TaskStatus::InProgress {
            let claim_active = other
                .active_claim
                .as_ref()
                .map(|c| !c.is_expired(current_epoch_ms))
                .unwrap_or(false);

            if claim_active {
                let overlap_files: Vec<&String> = other
                    .scope
                    .target_files
                    .iter()
                    .filter(|f| task.scope.target_files.contains(f))
                    .collect();
                if !overlap_files.is_empty() {
                    file_collision_warnings.push(format!(
                        "File collision with in-progress task '{}' ({}) on: {:?}",
                        other.ticket_key, other.title, overlap_files
                    ));
                }

                let overlap_symbols: Vec<&String> = other
                    .scope
                    .target_symbols
                    .iter()
                    .filter(|s| task.scope.target_symbols.contains(s))
                    .collect();
                if !overlap_symbols.is_empty() {
                    file_collision_warnings.push(format!(
                        "Symbol collision with in-progress task '{}' ({}) on: {:?}",
                        other.ticket_key, other.title, overlap_symbols
                    ));
                }
            }
        }

        // Check for deferred or triage bugs/tech-debt touching the same files/symbols
        if matches!(other.status, TaskStatus::Triage | TaskStatus::Deferred) {
            let files_overlap = other
                .scope
                .target_files
                .iter()
                .any(|f| task.scope.target_files.contains(f));
            let symbols_overlap = other
                .scope
                .target_symbols
                .iter()
                .any(|s| task.scope.target_symbols.contains(s));

            if files_overlap || symbols_overlap {
                known_deferred_issues_in_scope
                    .push((other.ticket_key.clone(), other.title.clone()));
            }
        }
    }

    TaskContextBriefing {
        task: task.clone(),
        verification_command,
        related_tasks_summary,
        inherited_decisions,
        file_collision_warnings,
        known_deferred_issues_in_scope,
    }
}
