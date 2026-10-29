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

use gaugeblock_contracts::{AnonymizedSynthesisTrace, SpoolEntry};

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
        let Ok(entry) = serde_json::from_str::<SpoolEntry>(line) else {
            continue;
        };
        if entry.trace.task_key.as_deref() == Some(task_key)
            && entry.trace.head_commit_sha.as_deref() == Some(head)
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

    let entry = SpoolEntry::from_trace(trace.clone(), crate::store::StorypoleStore::now_ms())
        .map_err(StorypoleError::Json)?;
    let line = serde_json::to_string(&entry).map_err(StorypoleError::Json)?;
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

/// Rewrites every line of `text` into a canonical `SpoolEntry` envelope.
///
/// The outbox briefly held two shapes (`GB-182`): `storypole` appended a bare trace while `joiner`
/// appended an envelope, so a reader expecting either shape choked on the other. This rewrites the
/// bare form with the canonical hash, keeping already-enveloped lines untouched.
///
/// Returns the rewritten text and how many bare-trace lines were converted. Blank lines are
/// dropped; lines that parse as neither shape are preserved verbatim so nothing is silently lost.
///
/// # Errors
///
/// [`StorypoleError::Json`] when an envelope cannot be rebuilt.
pub fn normalize_spool_text(text: &str) -> Result<(String, usize), StorypoleError> {
    let mut out = String::with_capacity(text.len());
    let mut migrated = 0usize;

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let rewritten = if let Ok(entry) = serde_json::from_str::<SpoolEntry>(line) {
            entry
        } else if let Ok(trace) = serde_json::from_str::<AnonymizedSynthesisTrace>(line) {
            migrated += 1;
            SpoolEntry::from_trace(trace, crate::store::StorypoleStore::now_ms())
                .map_err(StorypoleError::Json)?
        } else {
            out.push_str(line);
            out.push('\n');
            continue;
        };
        let encoded = serde_json::to_string(&rewritten).map_err(StorypoleError::Json)?;
        out.push_str(&encoded);
        out.push('\n');
    }

    Ok((out, migrated))
}

/// Rewrites legacy bare-trace lines in the spool file at `path`.
///
/// Returns how many lines were rewritten. The file is only touched when something actually changed,
/// which is what makes a second run a no-op.
///
/// # Errors
///
/// [`StorypoleError::Io`] when the spool cannot be read or written, or [`StorypoleError::Json`]
/// when an envelope cannot be rebuilt.
pub fn migrate_spool(path: &Path) -> Result<usize, StorypoleError> {
    if !path.exists() {
        return Ok(0);
    }

    let text = std::fs::read_to_string(path).map_err(StorypoleError::Io)?;
    let (out, migrated) = normalize_spool_text(&text)?;
    if migrated > 0 {
        std::fs::write(path, out).map_err(StorypoleError::Io)?;
    }
    Ok(migrated)
}
