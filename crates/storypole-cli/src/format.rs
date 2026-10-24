//! Human-friendly table and graph formatting utilities.

use std::collections::BTreeMap;
use storypole_core::model::events::TaskContextBriefing;
use storypole_core::model::task::{EdgeKind, TaskNode, TaskStatus};
use uuid::Uuid;

/// Format a list of tasks into an ASCII table with status and priority indicators.
pub fn format_task_table(tasks: &[TaskNode]) -> String {
    if tasks.is_empty() {
        return "No tasks found matching filter criteria.\n".to_string();
    }

    let mut out = String::new();
    out.push_str(&format!(
        "{:<8} {:<12} {:<3} {:<12} {:<16} {}\n",
        "KEY", "KIND", "PRI", "STATUS", "SCOPE", "TITLE"
    ));
    out.push_str(&"-".repeat(80));
    out.push('\n');

    for t in tasks {
        let kind = format!("{:?}", t.kind).to_lowercase();
        let status = format!("{:?}", t.status);
        let scope = t
            .scope
            .crate_name
            .as_deref()
            .or(t.scope.repo.as_deref())
            .unwrap_or("-");
        out.push_str(&format!(
            "{:<8} {:<12} P{:<2} {:<12} {:<16} {}\n",
            t.ticket_key, kind, t.priority, status, scope, t.title
        ));
    }
    out
}

/// Format a rich task context briefing into human-readable text.
pub fn format_task_briefing(b: &TaskContextBriefing) -> String {
    let t = &b.task;
    let mut out = String::new();
    out.push_str(&format!("Ticket:      {} ({})\n", t.ticket_key, t.id));
    out.push_str(&format!("Title:       {}\n", t.title));
    out.push_str(&format!(
        "Kind:        {:?} | Priority: P{}\n",
        t.kind, t.priority
    ));
    out.push_str(&format!("Status:      {:?}\n", t.status));

    if let Some(ref c) = t.scope.crate_name {
        out.push_str(&format!("Crate:       {c}\n"));
    }
    if let Some(ref r) = t.scope.repo {
        out.push_str(&format!("Repo:        {r}\n"));
    }
    if !t.scope.target_files.is_empty() {
        out.push_str(&format!("Files:       {:?}\n", t.scope.target_files));
    }
    if !t.scope.target_symbols.is_empty() {
        out.push_str(&format!("Symbols:     {:?}\n", t.scope.target_symbols));
    }
    if !t.scope.labels.is_empty() {
        out.push_str(&format!("Labels:      {:?}\n", t.scope.labels));
    }
    if let Some(ref cmd) = b.verification_command {
        out.push_str(&format!("Verify Cmd:  {cmd}\n"));
    }

    if let Some(ref claim) = t.active_claim {
        let branch_info = claim
            .git_branch
            .as_deref()
            .map(|b| format!(" [branch: {b}]"))
            .unwrap_or_default();
        out.push_str(&format!(
            "Claim:       {} (expires at epoch {}){}\n",
            claim.agent_id, claim.lease_expires_at_epoch_ms, branch_info
        ));
    }

    if let Some(ref receipt) = t.verification_receipt {
        let branch_str = receipt
            .git_branch
            .as_deref()
            .map(|b| format!(" [branch: {b}]"))
            .unwrap_or_default();
        let commits_str = if !receipt.commit_shas.is_empty() {
            format!(" [{} commit(s)]", receipt.commit_shas.len())
        } else {
            String::new()
        };
        out.push_str(&format!(
            "Receipt:     verified by {} (exit: {:?}){}{}\n",
            receipt.verifier, receipt.exit_code, branch_str, commits_str
        ));
    }

    if let Some(ref plan) = t.plan {
        out.push_str(&format!(
            "Plan:        {} ({} steps, file: {})\n",
            plan.summary,
            plan.steps.len(),
            plan.plan_file_path.as_deref().unwrap_or("inline")
        ));
        for step in &plan.steps {
            let mark = if step.completed { "[x]" } else { "[ ]" };
            let gate = step
                .verification_gate
                .as_deref()
                .map(|g| format!(" (gate: {g})"))
                .unwrap_or_default();
            out.push_str(&format!(
                "  {} {}: {}{}\n",
                mark, step.step_id, step.title, gate
            ));
        }
    }

    if !b.related_tasks_summary.is_empty() {
        out.push_str("\nRelated / Blocking Tasks:\n");
        for (key, title, status) in &b.related_tasks_summary {
            out.push_str(&format!("  * {key:<8} [{status:?}] {title}\n"));
        }
    }

    if !b.inherited_decisions.is_empty() {
        out.push_str("\nInherited Architecture Decisions:\n");
        for d in &b.inherited_decisions {
            out.push_str(&format!(
                "  * [{}] {}: {:?} ({})\n",
                d.category,
                d.question,
                d.chosen_option.as_deref().unwrap_or("none"),
                d.rationale.as_deref().unwrap_or("")
            ));
        }
    }

    if !b.file_collision_warnings.is_empty() {
        out.push_str("\nWARNINGS (Active File Collisions):\n");
        for w in &b.file_collision_warnings {
            out.push_str(&format!("  ! {w}\n"));
        }
    }

    if !b.known_deferred_issues_in_scope.is_empty() {
        out.push_str("\nDeferred Items in Scope:\n");
        for (key, title) in &b.known_deferred_issues_in_scope {
            out.push_str(&format!("  - {key}: {title}\n"));
        }
    }

    out
}

/// Render a Mermaid graph flowchart.
pub fn format_mermaid_graph(tasks: &BTreeMap<Uuid, TaskNode>) -> String {
    let mut out = String::from("flowchart TD\n");
    for t in tasks.values() {
        let style = match t.status {
            TaskStatus::Verified => ":::verified",
            TaskStatus::InProgress => ":::in_progress",
            TaskStatus::Ready => ":::ready",
            TaskStatus::Blocked => ":::blocked",
            _ => "",
        };
        out.push_str(&format!(
            "    {key}[\"{key}: {title}\"]{style}\n",
            key = t.ticket_key.replace('-', "_"),
            title = t.title.replace('"', "'")
        ));
    }
    for t in tasks.values() {
        for e in &t.edges {
            if let Some(target) = tasks.get(&e.target_id) {
                let arrow = match e.kind {
                    EdgeKind::BlockedBy => "-->|blocked by|",
                    EdgeKind::ParentEpic => "-->|subtask of|",
                    EdgeKind::RelatedTo => "-.->|related|",
                    EdgeKind::DiscoveredFrom => "-.->|discovered from|",
                };
                out.push_str(&format!(
                    "    {} {} {}\n",
                    t.ticket_key.replace('-', "_"),
                    arrow,
                    target.ticket_key.replace('-', "_")
                ));
            }
        }
    }
    out
}
