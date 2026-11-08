//! Strongly-typed domain error definitions for `storypole-core`.

use crate::model::TaskStatus;
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

/// Primary error enum for all Storypole operations.
#[derive(Debug, Error)]
pub enum StorypoleError {
    /// Standard I/O error with file context.
    #[error("I/O error at {path}: {source}")]
    IoWithPath {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Unspecified I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization or deserialization failure.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// TOML parsing or serialization failure.
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),

    /// SQLite database error.
    #[error("SQLite database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Task could not be resolved by ticket key, slug, or UUID.
    #[error("Task not found: '{0}'")]
    TaskNotFound(String),

    /// Given identifier is not a valid ticket key, slug, or UUID.
    #[error("Invalid task identifier: '{0}'")]
    InvalidIdentifier(String),

    /// A dependency edge would introduce a topological cycle in the DAG.
    #[error("Cycle detected in DAG: {0}")]
    CycleDetected(String),

    /// Decision not found for task.
    #[error("Decision not found with id {0}")]
    DecisionNotFound(Uuid),

    /// Illegal task status transition attempted.
    #[error("Invalid task status transition from {from:?} to {to:?}: {reason}")]
    InvalidTransition {
        from: TaskStatus,
        to: TaskStatus,
        reason: String,
    },

    /// An active lease claim is already held by another agent.
    #[error("Task {task_id} is already claimed by agent '{current_agent}' until {expires_at}")]
    ClaimConflict {
        task_id: Uuid,
        current_agent: String,
        expires_at: u64,
    },

    /// Claim lease has expired and cannot be renewed without re-claiming.
    #[error("Claim lease for task {0} has expired")]
    ClaimExpired(Uuid),

    /// Scope validation error (e.g. repo or crate not in allowed list).
    #[error("Scope validation failed: {0}")]
    ScopeViolation(String),

    /// An update would drop plan steps that are already complete (`GB-321`).
    ///
    /// Completion is evidence that something happened, so losing it is a decision rather than a side
    /// effect. The message names the steps, because "plan updated" while six recorded steps vanished is
    /// what this replaces.
    #[error(
        "updating the plan for {ticket} would drop completed step(s): {}; pass --force to drop them, or \
         use --step to keep them in the plan",
        steps.join(", ")
    )]
    PlanStepLoss { ticket: String, steps: Vec<String> },

    /// Verification failed or missing receipt.
    #[error("Verification error: {0}")]
    VerificationFailed(String),

    /// Verification of member work that the task does not say belongs to a member (`GB-316`).
    ///
    /// Refusing is the point. The alternative - certifying the harness for work that lives in a member -
    /// produces a receipt that is well-formed, exit-zero, and about the wrong tree; four tickets were
    /// certified that way before this existed (`GB-313`).
    #[error(
        "task {ticket} names {repo} work (from {derived_from}) but has no repository scope, so verifying \
         it would certify {harness} instead; set it with `storypole update {ticket} --repo {repo}` or \
         pass --repo explicitly"
    )]
    UnscopedMemberWork {
        ticket: String,
        repo: String,
        derived_from: String,
        harness: String,
    },

    /// Storypole repository uninitialized.
    #[error("Storypole repository not found at '{0}'. Run 'storypole init' first.")]
    Uninitialized(PathBuf),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Ledger file is corrupted or unreadable.
    #[error("Corrupt ledger at line {line}: {reason}")]
    CorruptLedger { line: usize, reason: String },
}
