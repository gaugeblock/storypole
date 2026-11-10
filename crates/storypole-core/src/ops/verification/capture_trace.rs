//! Spooling one Silver trace for a task that just verified.

use crate::error::StorypoleError;
use crate::model::decision::VerificationReceipt;
use crate::store::StorypoleStore;
use uuid::Uuid;

/// Spools one `Silver` trace for a task that just verified, keyed on `(task_key, head_commit_sha)`.
///
/// The record is deliberately scope-level rather than function-level: the task's target symbols and
/// the receipt are facts available here, while splitting a diff into functions needs `miter` and
/// belongs to a follow-up that can degrade gracefully. Re-verifying the same head writes nothing.
///
/// # Errors
///
/// [`StorypoleError::Io`] when the spool cannot be written, or [`StorypoleError::Json`] when the
/// record cannot be serialised.
pub(super) fn capture_trace(
    store: &StorypoleStore,
    task: &crate::model::task::TaskNode,
    receipt: &VerificationReceipt,
) -> Result<bool, StorypoleError> {
    // The suite's outbox, not this store's own: a trace belongs in the one corpus every consumer reads
    // (`SP-24`). Resolved in `telemetry` so the writer and `storypole spool` cannot disagree.
    let path = crate::telemetry::outbox_path(store);
    let passed = receipt.exit_code == Some(0);

    let trace = gaugeblock_contracts::AnonymizedSynthesisTrace {
        trace_id: Uuid::new_v4(),
        // Task scope carries no language yet; Rust is the deterministic default and the field is
        // overwritten by producers that know better (joiner, dado).
        language: gaugeblock_contracts::Language::Rust,
        tier: gaugeblock_contracts::TelemetryTier::Silver,
        selected_pattern_id: None,
        anonymized_scope_signature: format!("{} · {}", task.ticket_key, task.title),
        filled_holes: task
            .scope
            .target_symbols
            .iter()
            .map(|symbol| (symbol.clone(), "<verified>".to_string()))
            .collect(),
        rejected_attempt: None,
        verification: gaugeblock_contracts::VerificationReport {
            ast_valid: passed,
            typecheck_passed: passed,
            tests_passed: Some(passed),
            diagnostics: Vec::new(),
            duration_ms: 0,
        },
        task_key: Some(task.ticket_key.clone()),
        head_commit_sha: receipt.head_commit_sha.clone(),
    };

    crate::telemetry::append_trace(&path, &trace)
}
