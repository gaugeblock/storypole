//! Command handlers for `storypole plan` subcommands.

use crate::json_out::print_json;
use crate::plan_args::{PlanArgs, PlanCommands};
use anyhow::{Result, anyhow};
use std::fs;
use storypole_core::StorypoleStore;
use storypole_core::index::query::{get_task_by_id, resolve_task_id};
use storypole_core::model::plan::{PlanStep, TaskPlan};
use storypole_core::ops::plan_ops::{
    PlanUpdate, next_step, plan_progress, set_plan_step_complete, set_task_plan_with,
};

/// Dispatch `storypole plan` subcommands.
pub fn handle_plan(store: &mut StorypoleStore, args: PlanArgs, json: bool) -> Result<()> {
    match args.command {
        PlanCommands::Set {
            identifier,
            file,
            summary,
            steps,
            replace,
            force,
        } => {
            let id = resolve_task_id(store.conn(), &identifier)?;
            if !file.exists() {
                return Err(anyhow!("Plan file '{}' does not exist", file.display()));
            }

            let design_markdown = fs::read_to_string(&file)?;
            let plan_summary = summary.unwrap_or_else(|| {
                design_markdown
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .map(|l| l.trim_start_matches('#').trim().to_string())
                    .unwrap_or_else(|| "Execution Plan".to_string())
            });

            let mut parsed_steps = Vec::new();
            for s in steps {
                let parts: Vec<&str> = s.split('|').map(str::trim).collect();
                if parts.len() >= 2 {
                    let step_id = parts[0].to_string();
                    let title = parts[1].to_string();
                    let verification_gate = parts
                        .get(2)
                        .filter(|g| !g.is_empty())
                        .map(|g| g.to_string());
                    parsed_steps.push(PlanStep {
                        step_id,
                        title,
                        target_files: Vec::new(),
                        verification_gate,
                        completed: false,
                    });
                }
            }

            let plan = TaskPlan {
                summary: plan_summary,
                design_markdown,
                steps: parsed_steps,
                plan_file_path: None,
                updated_at_epoch_ms: 0,
            };

            // Merging is the default: a partial update must not erase the rest of the plan, and a
            // completed step is never dropped unless the caller asks for it (`GB-321`).
            let update = if replace {
                PlanUpdate::Replace
            } else {
                PlanUpdate::Merge
            };
            let updated_plan = set_task_plan_with(store, id, plan, update, force)
                .map_err(|e| anyhow!(e.to_string()))?;
            if json {
                print_json("plan", &serde_json::json!({"plan": updated_plan}))?;
            } else {
                let path_str = updated_plan
                    .plan_file_path
                    .as_deref()
                    .unwrap_or(".storypole/plans/<KEY>.md");
                let (done, total) = plan_progress(&updated_plan);
                println!(
                    "Attached plan to {identifier} with {total} steps ({done} complete). Saved to \
                     {path_str}"
                );
            }
        }
        PlanCommands::Step {
            identifier,
            step,
            undo,
        } => {
            let task_id = resolve_task_id(store.conn(), &identifier)?;
            let plan = set_plan_step_complete(store, task_id, &step, !undo)
                .map_err(|e| anyhow!(e.to_string()))?;
            if json {
                print_json("plan", &serde_json::json!({"plan": plan}))?;
            } else {
                let done = plan.steps.iter().filter(|s| s.completed).count();
                let state = if undo { "reopened" } else { "completed" };
                println!(
                    "Plan step `{step}` {state} on {identifier} ({done}/{} step(s) complete)",
                    plan.steps.len()
                );
            }
        }
        PlanCommands::Next { identifier } => {
            let id = resolve_task_id(store.conn(), &identifier)?;
            let task = get_task_by_id(store.conn(), id)?;
            let Some(plan) = task.plan.as_ref() else {
                return Err(anyhow!("No plan attached to {identifier}."));
            };
            let (done, total) = plan_progress(plan);
            let next = next_step(plan);
            if json {
                print_json(
                    "plan",
                    &serde_json::json!({
                        "ticket": task.ticket_key,
                        "completed": done,
                        "total": total,
                        "next": next.map(|step| serde_json::json!({
                            "step_id": step.step_id,
                            "title": step.title,
                            "verification_gate": step.verification_gate,
                        })),
                    }),
                )?;
            } else {
                match next {
                    Some(step) => {
                        println!("Next: {} - {}", step.step_id, step.title);
                        if let Some(gate) = step.verification_gate.as_deref() {
                            println!("Gate: {gate}");
                        }
                        println!("Progress: {done}/{total} step(s) complete");
                    }
                    // Every step complete is a statement about the plan, not an absence: saying "nothing
                    // found" would read as a failure to look (`GB-321`).
                    None => println!(
                        "Every step is complete on {identifier} ({done}/{total}); the plan is done."
                    ),
                }
            }
        }
        PlanCommands::Show { identifier } => {
            let id = resolve_task_id(store.conn(), &identifier)?;
            let task = get_task_by_id(store.conn(), id)?;
            if let Some(ref plan) = task.plan {
                if json {
                    print_json("plan", &serde_json::json!({"plan": plan}))?;
                } else {
                    println!("Ticket:  {}", task.ticket_key);
                    println!("Summary: {}", plan.summary);
                    if let Some(ref p) = plan.plan_file_path {
                        println!("File:    {p}");
                    }
                    if !plan.steps.is_empty() {
                        let (done, total) = plan_progress(plan);
                        println!("\nSteps ({done}/{total} complete):");
                        for step in &plan.steps {
                            let check = if step.completed { "[x]" } else { "[ ]" };
                            let gate = step
                                .verification_gate
                                .as_deref()
                                .map(|g| format!(" (gate: {g})"))
                                .unwrap_or_default();
                            println!("  {} {}: {}{}", check, step.step_id, step.title, gate);
                        }
                    }
                    println!("\n---\n{}", plan.design_markdown);
                }
            } else if json {
                println!("null");
            } else {
                println!("No plan attached to {identifier}.");
            }
        }
        PlanCommands::Collect { apply } => {
            let root_dir = store.root_dir().to_path_buf();
            let planned = crate::handlers::plan_collect::build(&root_dir)?;
            for unresolved in &planned.unresolved {
                println!("unresolved  {unresolved}");
            }
            if planned.moves.is_empty() {
                println!("nothing to collect: every pointer that can be resolved already is");
                return Ok(());
            }
            for entry in &planned.moves {
                println!(
                    "move  {}/{}  {} -> {}",
                    entry.store,
                    entry.ticket,
                    entry.from.display(),
                    entry.to.display()
                );
            }
            if !apply {
                println!(
                    "\n{} blueprint(s) could be placed. Nothing has been changed; re-run with --apply.",
                    planned.moves.len()
                );
                return Ok(());
            }
            let moved = crate::handlers::plan_collect::apply(&planned)?;
            let after = crate::handlers::plan_collect::build(&root_dir)?;
            println!(
                "\n{moved} blueprint(s) moved; {} pointer(s) unresolved",
                after.unresolved.len()
            );
        }
    }
    Ok(())
}
