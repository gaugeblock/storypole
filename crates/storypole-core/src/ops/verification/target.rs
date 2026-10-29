//! Locating a task's repository and shelling out to `miter verify`.

use crate::error::StorypoleError;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn resolve_target_dir(root: &Path, task: &crate::model::TaskNode) -> PathBuf {
    if let Some(repo) = task.scope.repo.as_deref() {
        let candidate = root.join(repo);
        if candidate.is_dir() {
            return candidate;
        }
    }
    if let Some(crate_name) = task.scope.crate_name.as_deref() {
        let base_crate = crate_name.split('-').next().unwrap_or(crate_name);
        let candidate = root.join(base_crate);
        if candidate.is_dir() {
            return candidate;
        }
    }
    root.to_path_buf()
}

pub(super) fn run_miter_verify(dir: &Path) -> Result<u32, StorypoleError> {
    let output = Command::new("miter")
        .arg("verify")
        .arg("--path")
        .arg(dir)
        .arg("--json")
        .output()
        .map_err(|e| StorypoleError::VerificationFailed(format!("Failed to invoke miter: {e}")))?;

    if !output.status.success() && output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(StorypoleError::VerificationFailed(format!(
            "miter verify failed: {stderr}"
        )));
    }

    let val: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|e| {
        StorypoleError::VerificationFailed(format!("Failed to parse miter JSON output: {e}"))
    })?;

    let outcome = val
        .get("outcome")
        .and_then(|o| o.as_str())
        .unwrap_or("unknown");
    if outcome == "success" {
        Ok(0)
    } else {
        let msg = val
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Compiler diagnostic failure");
        let spans_len = val
            .get("spans")
            .and_then(|s| s.as_array())
            .map(|a| a.len() as u32)
            .unwrap_or(1);
        Err(StorypoleError::VerificationFailed(format!(
            "miter compiler verification failed ({spans_len} diagnostic(s)): {msg}"
        )))
    }
}
