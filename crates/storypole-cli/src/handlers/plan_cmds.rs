//! Command handlers for `storypole plan` subcommands.

use crate::plan_args::{PlanArgs, PlanCommands};
use anyhow::{Result, anyhow};
use std::fs;
use storypole_core::StorypoleStore;
use storypole_core::index::query::{get_task_by_id, resolve_task_id};
use storypole_core::model::plan::{PlanStep, TaskPlan};
use storypole_core::ops::plan_ops::set_task_plan;

/// Dispatch `storypole plan` subcommands.
pub fn handle_plan(store: &mut StorypoleStore, args: PlanArgs, json: bool) -> Result<()> {
    match args.command {
        PlanCommands::Set {
            identifier,
            file,
            summary,
            steps,
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

            let updated_plan = set_task_plan(store, id, plan)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&updated_plan)?);
            } else {
                let path_str = updated_plan
                    .plan_file_path
                    .as_deref()
                    .unwrap_or(".storypole/plans/<KEY>.md");
                println!(
                    "Attached plan to {identifier} with {} steps. Saved to {path_str}",
                    updated_plan.steps.len()
                );
            }
        }
        PlanCommands::Show { identifier } => {
            let id = resolve_task_id(store.conn(), &identifier)?;
            let task = get_task_by_id(store.conn(), id)?;
            if let Some(ref plan) = task.plan {
                if json {
                    println!("{}", serde_json::to_string_pretty(plan)?);
                } else {
                    println!("Ticket:  {}", task.ticket_key);
                    println!("Summary: {}", plan.summary);
                    if let Some(ref p) = plan.plan_file_path {
                        println!("File:    {p}");
                    }
                    if !plan.steps.is_empty() {
                        println!("\nSteps:");
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
    }
    Ok(())
}
