//! Git inspection and branch checkout utilities for Storypole.

use std::path::Path;
use std::process::Command;

/// Resolve the current active git branch name (e.g. `feat/my-branch`), if in a git repository.
pub fn resolve_current_branch(dir: Option<&Path>) -> Option<String> {
    let mut cmd = Command::new("git");
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    cmd.args(["rev-parse", "--abbrev-ref", "HEAD"]);
    let out = cmd.output().ok()?;
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() && s != "HEAD" {
            return Some(s);
        }
    }
    None
}

/// Resolve the current git HEAD commit SHA-1 hash.
pub fn resolve_head_commit(dir: Option<&Path>) -> Option<String> {
    let mut cmd = Command::new("git");
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    cmd.args(["rev-parse", "HEAD"]);
    let out = cmd.output().ok()?;
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

/// Checkout an existing branch or create and checkout a new branch.
pub fn checkout_or_create_branch(dir: Option<&Path>, branch: &str) -> Result<String, String> {
    let mut cmd_checkout = Command::new("git");
    if let Some(d) = dir {
        cmd_checkout.current_dir(d);
    }
    cmd_checkout.args(["checkout", branch]);
    if let Ok(out) = cmd_checkout.output()
        && out.status.success()
    {
        return Ok(branch.to_string());
    }

    let mut cmd_create = Command::new("git");
    if let Some(d) = dir {
        cmd_create.current_dir(d);
    }
    cmd_create.args(["checkout", "-b", branch]);
    let out = cmd_create.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(branch.to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Resolve merge-base between a target base branch (e.g. `main`) and `head`.
pub fn resolve_merge_base(dir: Option<&Path>, base_branch: &str, head: &str) -> Option<String> {
    let mut cmd = Command::new("git");
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    cmd.args(["merge-base", base_branch, head]);
    if let Ok(out) = cmd.output()
        && out.status.success()
    {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }

    // Secondary attempt with origin/<base_branch>
    let mut cmd_origin = Command::new("git");
    if let Some(d) = dir {
        cmd_origin.current_dir(d);
    }
    cmd_origin.args(["merge-base", &format!("origin/{base_branch}"), head]);
    if let Ok(out) = cmd_origin.output()
        && out.status.success()
    {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }

    None
}

/// Resolve all commit SHAs comprising the delta between `base_sha` and `head_sha`.
pub fn resolve_commit_shas_between(
    dir: Option<&Path>,
    base_sha: &str,
    head_sha: &str,
) -> Vec<String> {
    if base_sha == head_sha {
        return vec![head_sha.to_string()];
    }

    let mut cmd = Command::new("git");
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    cmd.args(["rev-list", "--reverse", &format!("{base_sha}..{head_sha}")]);
    if let Ok(out) = cmd.output()
        && out.status.success()
    {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let list: Vec<String> = stdout
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if !list.is_empty() {
            return list;
        }
    }

    vec![head_sha.to_string()]
}
