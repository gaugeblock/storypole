//! Renders a task's own pending decisions for the human briefing (`SP-4`).

use storypole_core::model::decision::{DecisionStatus, TaskDecision};

/// Renders the task's own decisions, unresolved first.
///
/// Above anyone else's, and with the candidate answers printed, because an unresolved decision is a
/// question the operator has to answer before the work can proceed: printing the question without the
/// options makes the briefing ask something it does not let you reply to. These decisions were never
/// rendered at all - the briefing showed only decisions inherited from linked tasks (`SP-4`).
pub(crate) fn render_task_decisions(out: &mut String, decisions: &[TaskDecision]) {
    if decisions.is_empty() {
        return;
    }
    out.push_str("Decisions On This Task:\n");
    let mut ordered: Vec<&TaskDecision> = decisions.iter().collect();
    ordered.sort_by_key(|decision| decision.status == DecisionStatus::Resolved);
    for decision in ordered {
        match decision.chosen_option.as_deref() {
            Some(choice) => out.push_str(&format!(
                "  * [{}] {}: {choice} ({})\n",
                decision.category,
                decision.question,
                decision.rationale.as_deref().unwrap_or("")
            )),
            None => {
                out.push_str(&format!(
                    "  ? [{}] {}\n",
                    decision.category, decision.question
                ));
                for option in &decision.proposed_options {
                    out.push_str(&format!("      - {option}\n"));
                }
            }
        }
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn decision(
        question: &str,
        status: DecisionStatus,
        chosen: Option<&str>,
        options: &[&str],
    ) -> TaskDecision {
        TaskDecision {
            decision_id: Uuid::nil(),
            parent_decision_id: None,
            status,
            category: "storage".to_string(),
            question: question.to_string(),
            proposed_options: options.iter().map(|option| (*option).to_string()).collect(),
            chosen_option: chosen.map(str::to_string),
            rationale: chosen.map(|_| "chosen because it is reversible".to_string()),
            resolved_by: None,
            updated_at_epoch_ms: 0,
        }
    }

    #[test]
    fn pending_decision_prints_its_options() {
        let mut out = String::new();
        render_task_decisions(
            &mut out,
            &[decision(
                "which store?",
                DecisionStatus::Pending,
                None,
                &["redb", "sqlite"],
            )],
        );
        assert!(out.contains("? [storage] which store?"));
        assert!(out.contains("- redb") && out.contains("- sqlite"));
    }

    #[test]
    fn unresolved_decisions_come_before_resolved_ones() {
        let mut out = String::new();
        render_task_decisions(
            &mut out,
            &[
                decision("settled", DecisionStatus::Resolved, Some("redb"), &[]),
                decision("open", DecisionStatus::Pending, None, &["a"]),
            ],
        );
        let open = out.find("open").unwrap_or(usize::MAX);
        let settled = out.find("settled").unwrap_or(usize::MAX);
        assert!(
            open < settled,
            "a question to answer outranks a decision already made"
        );
    }

    #[test]
    fn a_task_with_no_decisions_adds_nothing() {
        let mut out = String::new();
        render_task_decisions(&mut out, &[]);
        assert!(out.is_empty());
    }
}
