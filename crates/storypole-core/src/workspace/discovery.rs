//! Member repository discovery for multi-repo workspaces.

use crate::config::StorypoleConfig;
use crate::workspace::config::WorkspaceConfig;
use gaugeblock_workspace::WorkspaceTopology;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A member repository discovered within a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredMember {
    /// Repository name (e.g. "benchwork", "miter").
    pub name: String,
    /// Absolute path to repository root.
    pub repo_path: PathBuf,
    /// Path to member's `.storypole` directory, if initialized.
    pub dot_dir: Option<PathBuf>,
    /// Ticket prefix (e.g. "BW", "MTR"), if configured.
    pub ticket_prefix: Option<String>,
}

/// Discover member repositories in `workspace_root`.
pub fn discover_members(
    workspace_root: &Path,
    config: Option<&WorkspaceConfig>,
) -> Vec<DiscoveredMember> {
    let mut members = Vec::new();

    if let Some(cfg) = config
        && !cfg.members.is_empty()
        && cfg.members != ["*"]
    {
        for item in &cfg.members {
            let candidate = workspace_root.join(item);
            if candidate.is_dir() {
                let name = candidate
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(item.as_str())
                    .to_string();
                members.push(inspect_member(&name, candidate));
            }
        }
        members.sort_by(|a, b| a.name.cmp(&b.name));
        return members;
    }

    // Auto-scan immediate child directories. The scan itself comes from `gaugeblock-workspace`
    // (`GB-189`) so the suite root and the member list have one definition; the qualification rule
    // stays here, because storypole accepts a member that holds `.storypole` or `Cargo.toml` even
    // when it is not a checkout. Narrowing to `.git` would break a standalone workspace whose
    // directories were never git repositories, which is a use storypole supports today.
    let topology = WorkspaceTopology::discover_with(workspace_root, is_storypole_member);
    topology
        .members()
        .iter()
        .map(|member| inspect_member(&member.name, member.path.clone()))
        .collect()
}

/// Whether a directory counts as a storypole workspace member.
///
/// Hidden directories are filtered by [`WorkspaceTopology::discover_with`] before this is called.
fn is_storypole_member(path: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|n| n.to_str())
        && (name == "target" || name == "scripts" || name == "docs")
    {
        return false;
    }
    // Both layouts: a member is discovered by its store, and the canonical one lives under `.gaugeblock`
    // since GB-344, so checking only `.storypole` silently stopped finding members.
    path.join(".storypole").is_dir()
        || path.join(".gaugeblock").join("storypole").is_dir()
        || path.join(".git").is_dir()
        || path.join(".git").is_file()
        || path.join("Cargo.toml").is_file()
}

fn inspect_member(name: &str, repo_path: PathBuf) -> DiscoveredMember {
    let sp_dir = repo_path.join(".storypole");
    let gb_dir = repo_path.join(".gaugeblock").join("storypole");

    let dot_dir = if sp_dir.is_dir() {
        Some(sp_dir)
    } else if gb_dir.is_dir() {
        Some(gb_dir)
    } else {
        None
    };

    let ticket_prefix = dot_dir.as_ref().and_then(|dd| {
        let cfg_path = dd.join("config.toml");
        StorypoleConfig::load_unified_or_legacy(&repo_path, &cfg_path)
            .ok()
            .map(|c| c.ticket_prefix)
    });

    DiscoveredMember {
        name: name.to_string(),
        repo_path,
        dot_dir,
        ticket_prefix,
    }
}
