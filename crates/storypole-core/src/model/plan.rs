//! Task plan and step definitions.

use serde::{Deserialize, Serialize};

/// Granular step within an architectural or execution plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    /// Identifier of the step (e.g. `S1`, `step_1`).
    pub step_id: String,
    /// Human-readable title or objective of the step.
    pub title: String,
    /// Target source or test files touched in this step.
    #[serde(default)]
    pub target_files: Vec<String>,
    /// Verification gate command or test required to pass this step.
    #[serde(default)]
    pub verification_gate: Option<String>,
    /// Whether this step has been completed.
    #[serde(default)]
    pub completed: bool,
}

/// Structured execution plan and design markdown blueprint attached to a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskPlan {
    /// High-level executive summary of the plan.
    pub summary: String,
    /// Complete, unabridged technical design specification in Markdown.
    pub design_markdown: String,
    /// Ordered execution steps.
    #[serde(default)]
    pub steps: Vec<PlanStep>,
    /// Relative path to persisted plan markdown file (e.g. `.storypole/plans/<KEY>.md`).
    #[serde(default)]
    pub plan_file_path: Option<String>,
    /// Epoch millisecond timestamp when the plan was last updated.
    pub updated_at_epoch_ms: u64,
}
