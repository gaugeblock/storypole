//! Member repository discovery for multi-repo workspaces.

use crate::config::StorypoleConfig;
use crate::workspace::config::WorkspaceConfig;
use serde::{Deserialize, Serialize};
use std::fs;
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

    // Auto-scan immediate child directories
    if let Ok(entries) = fs::read_dir(workspace_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let Some(dir_name_str) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };

            // Skip hidden, metadata, and build directories
            if dir_name_str.starts_with('.')
                || dir_name_str == "target"
                || dir_name_str == "scripts"
                || dir_name_str == "docs"
            {
                continue;
            }

            let dir_name = dir_name_str.to_string();

            // Treat directory as a member if it has .storypole, .git, or Cargo.toml
            let has_storypole = path.join(".storypole").is_dir();
            let has_git = path.join(".git").is_dir() || path.join(".git").is_file();
            let has_cargo = path.join("Cargo.toml").is_file();

            if has_storypole || has_git || has_cargo {
                members.push(inspect_member(&dir_name, path));
            }
        }
    }

    members.sort_by(|a, b| a.name.cmp(&b.name));
    members
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
