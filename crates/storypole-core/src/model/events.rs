//! Ledger events and context briefing models.

use crate::model::decision::{AgentClaim, HandoffPacket, TaskDecision, VerificationReceipt};
use crate::model::task::{TaskEdge, TaskNode, TaskScope, TaskStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Assembled context payload returned by `storypole next` / `storypole claim` / MCP tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskContextBriefing {
    /// The primary task under inspection or claim.
    pub task: TaskNode,
    /// Resolved shell verification command from `.storypole/config.toml`.
    pub verification_command: Option<String>,
    /// Ticket key, title, and status for related and blocking tasks.
    pub related_tasks_summary: Vec<(String, String, TaskStatus)>,
    /// Relevant resolved architectural decisions inherited from parent epics or linked tasks.
    pub inherited_decisions: Vec<TaskDecision>,
    /// Warnings if any other `InProgress` task claims overlapping files or symbols.
    pub file_collision_warnings: Vec<String>,
    /// Triage or Deferred items whose target files or symbols overlap this task's scope.
    pub known_deferred_issues_in_scope: Vec<(String, String)>,
}

/// Immutable append-only event written to `.storypole/ledger.jsonl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum LedgerEvent {
    /// Initial creation of a backlog task.
    TaskCreated { task: TaskNode },

    /// In-place attribute updates.
    TaskUpdated {
        task_id: Uuid,
        title: Option<String>,
        description: Option<String>,
        priority: Option<u8>,
        scope: Option<TaskScope>,
        acceptance_criteria: Option<Vec<String>>,
        timestamp_ms: u64,
    },

    /// Transition between task lifecycle states.
    StatusChanged {
        task_id: Uuid,
        from: TaskStatus,
        to: TaskStatus,
        reason: Option<String>,
        timestamp_ms: u64,
    },

    /// Directed edge added between tasks.
    EdgeAdded {
        task_id: Uuid,
        edge: TaskEdge,
        timestamp_ms: u64,
    },

    /// Directed edge removed between tasks.
    EdgeRemoved {
        task_id: Uuid,
        edge: TaskEdge,
        timestamp_ms: u64,
    },

    /// A decision requirement raised on a task.
    DecisionRaised {
        task_id: Uuid,
        decision: TaskDecision,
        timestamp_ms: u64,
    },

    /// A decision resolved with chosen option and rationale.
    DecisionResolved {
        task_id: Uuid,
        decision_id: Uuid,
        chosen_option: String,
        rationale: String,
        resolved_by: String,
        spawned_followup: Option<TaskDecision>,
        appended_criterion: Option<String>,
        timestamp_ms: u64,
    },

    /// A previously resolved decision superseded.
    DecisionSuperseded {
        task_id: Uuid,
        decision_id: Uuid,
        reason: String,
        timestamp_ms: u64,
    },

    /// Agent acquired lease claim on task.
    TaskClaimed { task_id: Uuid, claim: AgentClaim },

    /// Agent released lease claim on task.
    TaskReleased {
        task_id: Uuid,
        agent_id: String,
        timestamp_ms: u64,
    },

    /// Handoff packet recorded upon pause, yield, or failure.
    HandoffRecorded {
        task_id: Uuid,
        handoff: HandoffPacket,
        new_status: TaskStatus,
    },

    /// Large task split into child tasks under an Epic.
    TaskSplit {
        parent_task_id: Uuid,
        spawned_children: Vec<TaskNode>,
        timestamp_ms: u64,
    },

    /// Task successfully passed verification.
    TaskVerified {
        task_id: Uuid,
        receipt: VerificationReceipt,
    },

    /// Deferred bug, tech debt, or enhancement captured out-of-band.
    DeferredItemCaptured {
        task: TaskNode,
        captured_by_agent: String,
        discovered_from_task_id: Option<Uuid>,
    },
}
