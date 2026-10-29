//! Local training-trace spool: append-only JSONL beside the canonical `.gaugeblock/`.
//!
//! `storypole verify` is the one place a task becomes Verified, and it is therefore the one place
//! where `base_commit_sha`, `head_commit_sha`, the receipt and the task context are all known at
//! once (`GB-165`). `outfeed gate` is deliberately *not* a writer: it is a stateless, repeatable
//! check that agents run several times mid-branch, so writing there would emit duplicate traces for
//! unfinished work.
//!
//! The record is `gaugeblock_contracts::AnonymizedSynthesisTrace` - the same type `joiner` spools and
//! `rabbet` mines (`GB-174`) - and idempotency is keyed on `(task_key, head_commit_sha)`.

use std::path::{Path, PathBuf};

use gaugeblock_contracts::AnonymizedSynthesisTrace;

use crate::error::StorypoleError;

/// Resolves `<canonical .gaugeblock>/spool/outbox.jsonl` from the store's dot directory.
///
/// The spool lives beside `storypole/` rather than inside it, so every worktree and every tool
/// resolves one outbox. The directory is gitignored: it is derived local telemetry.
#[must_use]
pub fn spool_path(dot_dir: &Path) -> PathBuf {
    dot_dir
        .parent()
        .unwrap_or(dot_dir)
        .join("spool")
        .join("outbox.jsonl")
}

/// `true` when a trace for this `(task_key, head_commit_sha)` is already spooled.
///
/// # Errors
///
/// [`StorypoleError::Io`] when the spool exists but cannot be read.
pub fn already_spooled(
    path: &Path,
    task_key: Option<&str>,
    head_commit_sha: Option<&str>,
) -> Result<bool, StorypoleError> {
    let (Some(task_key), Some(head)) = (task_key, head_commit_sha) else {
        return Ok(false);
    };
    if !path.exists() {
        return Ok(false);
    }

    let text = std::fs::read_to_string(path).map_err(StorypoleError::Io)?;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<AnonymizedSynthesisTrace>(line) else {
            continue;
        };
        if record.task_key.as_deref() == Some(task_key)
            && record.head_commit_sha.as_deref() == Some(head)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Appends `trace` to the spool unless an equivalent record is already present.
///
/// Returns `true` when a line was written and `false` when the trace was already there, which is
/// what makes re-verifying the same head a no-op rather than a duplicate.
///
/// # Errors
///
/// [`StorypoleError::Io`] when the spool directory cannot be created or the file cannot be appended.
pub fn append_trace(path: &Path, trace: &AnonymizedSynthesisTrace) -> Result<bool, StorypoleError> {
    if already_spooled(
        path,
        trace.task_key.as_deref(),
        trace.head_commit_sha.as_deref(),
    )? {
        return Ok(false);
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(StorypoleError::Io)?;
    }

    let line = serde_json::to_string(trace).map_err(StorypoleError::Json)?;
    let mut existing = String::new();
    if path.exists() {
        existing = std::fs::read_to_string(path).map_err(StorypoleError::Io)?;
    }
    let needs_newline = !existing.is_empty() && !existing.ends_with('\n');
    let mut payload = existing;
    if needs_newline {
        payload.push('\n');
    }
    payload.push_str(&line);
    payload.push('\n');

    std::fs::write(path, payload).map_err(StorypoleError::Io)?;
    Ok(true)
}
