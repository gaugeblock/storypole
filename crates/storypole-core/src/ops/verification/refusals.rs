//! What `verify` refuses to certify (`GB-334`).
//!
//! Each refusal comes from a measured failure rather than a preference. The first is the one that matters
//! most: `GB-57` was closed `Verified` with all six of its plan steps unchecked and a manual receipt, and
//! nothing objected - the epic described a fleet of distributed ledgers, no ledger existed, and the record
//! said done. A plan is the ticket's own statement of what finishing means, so verifying past an unfinished
//! plan is verifying something other than the ticket.

use gaugeblock_contracts::TaskNode;

use crate::error::StorypoleError;

/// Refuses verification while the plan still has unchecked steps.
///
/// A task without a plan passes: the plan is an optional, stronger promise, and requiring one would punish
/// the honest small ticket. A plan whose steps are *all* complete also passes, which is the ordinary case.
///
/// # Errors
///
/// [`StorypoleError::PlanIncomplete`] naming every unchecked step, because "plan incomplete" without the list
/// sends the reader looking for which one.
pub fn unchecked_plan_steps(task: &TaskNode) -> Result<(), StorypoleError> {
    let Some(plan) = task.plan.as_ref() else {
        return Ok(());
    };
    let pending: Vec<String> = plan
        .steps
        .iter()
        .filter(|step| !step.completed)
        .map(|step| step.step_id.clone())
        .collect();
    if pending.is_empty() {
        return Ok(());
    }
    Err(StorypoleError::PlanIncomplete {
        ticket: task.ticket_key.clone(),
        steps: pending,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use gaugeblock_contracts::{PlanStep, TaskKind, TaskPlan, TaskScope, TaskStatus};

    use super::*;

    fn task_with_plan(steps: Vec<PlanStep>) -> TaskNode {
        TaskNode {
            id: uuid::Uuid::from_u128(1),
            ticket_key: "GB-1".to_string(),
            slug: "gb-1".to_string(),
            title: "a ticket".to_string(),
            description: String::new(),
            kind: TaskKind::Feature,
            status: TaskStatus::InProgress,
            priority: 3,
            scope: TaskScope::default(),
            edges: Vec::new(),
            decisions: Vec::new(),
            acceptance_criteria: Vec::new(),
            plan: Some(TaskPlan {
                summary: "a plan".to_string(),
                design_markdown: String::new(),
                steps,
                plan_file_path: None,
                updated_at_epoch_ms: 1,
            }),
            active_claim: None,
            latest_handoff: None,
            verification_receipt: None,
            created_at_epoch_ms: 1,
            updated_at_epoch_ms: 1,
            archived_at_epoch_ms: None,
        }
    }

    fn step(id: &str, completed: bool) -> PlanStep {
        PlanStep {
            step_id: id.to_string(),
            title: format!("do {id}"),
            target_files: Vec::new(),
            verification_gate: None,
            completed,
        }
    }

    #[test]
    fn a_task_without_a_plan_is_not_refused() {
        // The plan is an optional, stronger promise; requiring one would punish the honest small ticket.
        let mut task = task_with_plan(Vec::new());
        task.plan = None;
        assert!(unchecked_plan_steps(&task).is_ok());
    }

    #[test]
    fn a_finished_plan_is_not_refused() {
        let task = task_with_plan(vec![step("step-1", true), step("step-2", true)]);
        assert!(unchecked_plan_steps(&task).is_ok());
    }

    #[test]
    fn an_unfinished_plan_names_every_step_that_is_open() {
        // The GB-57 shape: verified with 0 of 6, and the record claimed delivery.
        let task = task_with_plan(vec![
            step("step-1", false),
            step("step-2", true),
            step("step-3", false),
        ]);

        let error = unchecked_plan_steps(&task).expect_err("unfinished plan");
        let message = error.to_string();
        assert!(message.contains("GB-1"), "{message}");
        assert!(message.contains("step-1"), "{message}");
        assert!(message.contains("step-3"), "{message}");
        assert!(
            !message.contains("step-2"),
            "finished steps are not listed: {message}"
        );
    }
}
