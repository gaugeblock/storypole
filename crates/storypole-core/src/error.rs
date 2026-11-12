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

    /// A request was refused on its merits, and no more specific variant fits (`GB-242`).
    ///
    /// The plan-step commands used [`Self::InvalidIdentifier`], which prefixed a correct message with a
    /// wrong claim about *what* was invalid: `storypole plan step SP-1 99` answered "Invalid task
    /// identifier: 'no plan step `99` on SP-1 (7 step(s) recorded)'", blaming the identifier for a step
    /// number that is a perfectly good ticket key away from being a plain "no". Rendered verbatim, because
    /// the message is the error and a prefix that contradicts it is worse than none.
    #[error("{0}")]
    Rejected(String),

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

    /// Verification was asked for while the task's own plan still has open steps (`GB-334`).
    #[error(
        "{ticket} has {count} unchecked plan step(s) ({steps}); an unfinished plan is an unfinished ticket, so          either complete the steps or retire them before verifying",
        count = steps.len(),
        steps = steps.join(", ")
    )]
    PlanIncomplete { ticket: String, steps: Vec<String> },

    /// An epic was verified while children of it are still open (`GB-334`).
    #[error(
        "{ticket} is an epic with {count} unfinished child task(s) ({children}); an epic is a promise that the          work is done, so close the children or retire them first",
        count = children.len(),
        children = children.join(", ")
    )]
    EpicHasOpenChildren {
        ticket: String,
        children: Vec<String>,
    },

    /// A manual receipt was offered for a task that has an executable verification chain (`GB-334`).
    #[error(
        "{ticket} has an executable verification chain ({chain}); a manual receipt attests less than the\n         chain already could, so run the chain or say --verifier explicitly if it cannot run"
    )]
    ManualReceiptWithChain { ticket: String, chain: String },

    /// A migration's records did not reproduce the ledger they came from (`GB-331`).
    #[error("migration divergence for {ticket}: {detail}")]
    MigrationDivergence { ticket: String, detail: String },

    /// A ticket record could not be read, written or locked (`GB-329`).
    #[error("Ticket record at '{path}': {reason}")]
    TicketRecord { path: PathBuf, reason: String },

    /// A verification was asked for against a repository that is not checked out here (`GB-313`).
    ///
    /// The alternative was the silent one: `resolve_target_dir` falls back to the harness root when
    /// `root/<repo>` is absent, the chain runs there, and the receipt stamps the repository the ticket
    /// *declared* - so the record names a tree nothing was run against.
    #[error(
        "{ticket} is scoped to {repo}, which is not checked out at {path}: the chain would have run \
         against another tree and the receipt would name one it never touched"
    )]
    RepositoryNotCheckedOut {
        ticket: String,
        repo: String,
        path: PathBuf,
    },

    /// A status transition was requested where it would hide or misreport work (`SP-47`).
    #[error("{ticket}: cannot move {from:?} -> {to:?}, {why}")]
    IllegalTransition {
        ticket: String,
        from: TaskStatus,
        to: TaskStatus,
        why: String,
    },
}
