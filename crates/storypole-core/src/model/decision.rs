//! Decision, handoff, lease claim, and verification receipt types.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Lifecycle state for an attached decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStatus {
    /// Awaits resolution from a human or authority agent.
    Pending,
    /// Resolved with a selected option and justification.
    Resolved,
    /// Previously resolved decision rendered obsolete or overridden.
    Superseded,
}

/// A decision node attached to a task, supporting chained follow-up decisions and reversal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDecision {
    /// Unique identifier for this decision.
    pub decision_id: Uuid,
    /// Optional parent decision when decisions are chained sequentially.
    pub parent_decision_id: Option<Uuid>,
    /// Status of this decision.
    pub status: DecisionStatus,
    /// Domain category (e.g. `architecture`, `api_design`, `error_handling`).
    pub category: String,
    /// Question or dilemma to resolve.
    pub question: String,
    /// Candidate options proposed for consideration.
    pub proposed_options: Vec<String>,
    /// Selected option chosen upon resolution.
    pub chosen_option: Option<String>,
    /// Rationale and justification for the decision.
    pub rationale: Option<String>,
    /// Identifier of the agent or human who resolved the decision.
    pub resolved_by: Option<String>,
    /// Milliseconds since Unix epoch when last updated.
    pub updated_at_epoch_ms: u64,
}

/// Structured continuity checkpoint recorded when an agent pauses, yields, or fails mid-task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffPacket {
    /// Identifier of the agent writing this handoff.
    pub recorded_by_agent: String,
    /// Milliseconds since Unix epoch when recorded.
    pub timestamp_ms: u64,
    /// Checklist items successfully finished during this session.
    pub completed_items: Vec<String>,
    /// Remaining checklist items required to complete the task.
    pub remaining_items: Vec<String>,
    /// Specific failure reason, compiler error, or question blocking progress.
    pub last_error_or_blocker: Option<String>,
    /// Active git commit hash or branch name.
    pub git_ref: Option<String>,
    /// Working tree files modified during the session.
    pub dirty_files: Vec<String>,
}

/// Active agent claim lock with lease expiry semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentClaim {
    /// Identifier of the claiming agent (e.g. `sawhorse:worker-1`, `human:colin`).
    pub agent_id: String,
    /// Milliseconds since Unix epoch when claim was acquired.
    pub claimed_at_epoch_ms: u64,
    /// Milliseconds since Unix epoch after which the lease expires without heartbeat.
    pub lease_expires_at_epoch_ms: u64,
}

impl AgentClaim {
    /// Check whether this claim has expired relative to current epoch time in milliseconds.
    pub fn is_expired(&self, current_epoch_ms: u64) -> bool {
        current_epoch_ms >= self.lease_expires_at_epoch_ms
    }
}

/// Cryptographic or process receipt certifying a task has passed verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationReceipt {
    /// System or tool that ran verification (e.g. `miter-engine`, `cargo-test`, `human`).
    pub verifier: String,
    /// Exact command or suite executed.
    pub command: String,
    /// Process exit code (`0` indicates success).
    pub exit_code: Option<i32>,
    /// Count of compiler warnings or diagnostics emitted.
    pub diagnostic_count: Option<u32>,
    /// Execution output summary or stdout/stderr snippet.
    pub summary: String,
    /// Milliseconds since Unix epoch when verification succeeded.
    pub verified_at_epoch_ms: u64,
}
