//! Core task and graph model definitions for `storypole-core`.

use crate::model::decision::{AgentClaim, HandoffPacket, TaskDecision, VerificationReceipt};
use crate::model::plan::TaskPlan;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Distinct categorization of backlog items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// High-level umbrella task composed of multiple subtasks.
    Epic,
    /// Standard feature implementation.
    #[default]
    Feature,
    /// Code quality, modularization, or architectural improvements.
    Refactor,
    /// Software defect requiring remediation.
    Bug,
    /// Technical debt, missing AST coverage, or cleanup.
    TechDebt,
    /// Architectural research, benchmarking, or exploration spike.
    Spike,
}

/// Lifecycle state machine for a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Captured via `storypole defer`; awaits human or `chalkline` triage before entering `Ready`.
    Triage,
    /// Blocked by unverified `blocked_by` tasks or unresolved `Pending` decisions.
    Blocked,
    /// All hard dependencies verified and zero pending decisions; eligible for `storypole next`.
    Ready,
    /// Actively claimed by an agent or human under an active lease.
    InProgress,
    /// Active AST pattern/hole synthesis in progress via `joiner`.
    Synthesizing,
    /// Running compiler / test verification via `miter`.
    Verifying,
    /// Passed verification with a recorded `VerificationReceipt`.
    Verified,
    /// Completed, certified, and archived out of active backlog views.
    Archived,
    /// Intentionally parked for future milestones.
    Deferred,
    /// Exhausted retry budget or failed verification; requires handoff inspection or split.
    Failed,
}

/// Semantic relationship type between tasks in the DAG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// Hard topological execution lock: target cannot enter `Ready` until source is `Verified`.
    BlockedBy,
    /// Soft bi-directional link: surfaces sibling summary & decisions in context briefings.
    RelatedTo,
    /// Provenance link: created automatically when an agent defers an issue during an active task.
    DiscoveredFrom,
    /// Hierarchical grouping under a parent Epic or split task.
    ParentEpic,
}

/// Directed edge connecting two tasks in the storypole DAG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskEdge {
    /// Type of relationship.
    pub kind: EdgeKind,
    /// Unique identifier of the target task.
    pub target_id: Uuid,
    /// Optional cached ticket key for standalone or cross-repo rendering (e.g. "MTR-12").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_ticket_key: Option<String>,
    /// Optional target repository name if cross-repository (e.g. "miter").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_repo: Option<String>,
}

impl TaskEdge {
    /// Create a standard task edge without target metadata.
    pub fn new(kind: EdgeKind, target_id: Uuid) -> Self {
        Self {
            kind,
            target_id,
            target_ticket_key: None,
            target_repo: None,
        }
    }

    /// Attach cached target metadata for cross-repo reference.
    pub fn with_target_meta(mut self, ticket_key: impl Into<String>, repo: Option<impl Into<String>>) -> Self {
        self.target_ticket_key = Some(ticket_key.into());
        self.target_repo = repo.map(|r| r.into());
        self
    }
}

/// Multi-dimensional scope coordinates for filtering across repos, crates, and AST symbols.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskScope {
    /// Target repository name.
    pub repo: Option<String>,
    /// Target Rust crate or package name.
    pub crate_name: Option<String>,
    /// Sub-module or file path hierarchy.
    pub module_path: Option<String>,
    /// Specific file paths touched or governed by this task.
    pub target_files: Vec<String>,
    /// Specific AST symbol names (e.g. `crate::module::Type`) touched by this task.
    pub target_symbols: Vec<String>,
    /// Arbitrary domain labels/tags (e.g. `cli`, `ast`, `urgent`).
    pub labels: Vec<String>,
}

/// Canonical task node persisted in the ledger and materialized in SQLite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskNode {
    /// Globally unique task identifier.
    pub id: Uuid,
    /// Human-friendly sequential key (e.g., `SP-14`, `GB-3`).
    pub ticket_key: String,
    /// Deterministic URL/CLI-friendly slug (e.g., `implement-dag-cycle-check`).
    pub slug: String,
    /// Concise summary line.
    pub title: String,
    /// Detailed description and background context.
    pub description: String,
    /// Backlog item category.
    pub kind: TaskKind,
    /// Current workflow lifecycle state.
    pub status: TaskStatus,
    /// Priority level: 1 (highest / critical) to 5 (lowest / trivial).
    pub priority: u8,
    /// Granular scope coordinates.
    pub scope: TaskScope,
    /// Directed relationships to other tasks.
    pub edges: Vec<TaskEdge>,
    /// Attached architectural / implementation decisions.
    pub decisions: Vec<TaskDecision>,
    /// Concrete criteria required to pass verification.
    pub acceptance_criteria: Vec<String>,
    /// Active agent lease lock, if claimed.
    pub active_claim: Option<AgentClaim>,
    /// Most recent handoff packet recorded on pause or yield.
    pub latest_handoff: Option<HandoffPacket>,
    /// Verification receipt recorded upon passing gate.
    pub verification_receipt: Option<VerificationReceipt>,
    /// Attached execution plan and design markdown blueprint.
    #[serde(default)]
    pub plan: Option<TaskPlan>,
    /// Milliseconds since Unix epoch when created.
    pub created_at_epoch_ms: u64,
    /// Milliseconds since Unix epoch when last updated.
    pub updated_at_epoch_ms: u64,
}
