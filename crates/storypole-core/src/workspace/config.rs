//! Workspace configuration schema and parser.

use crate::error::StorypoleError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Configuration defining multi-repository workspace membership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkspaceConfig {
    /// List of member repository paths or names (e.g. `["benchwork", "miter"]`).
    #[serde(default)]
    pub members: Vec<String>,
    /// Default ticket prefix for workspace-level / cross-cutting tasks.
    #[serde(default)]
    pub default_prefix: Option<String>,
}

impl WorkspaceConfig {
    /// Attempt to load workspace config from `.gaugeblock/config.toml` or `.storypole/config.toml`.
    pub fn load_from_root(root_dir: &Path) -> Result<Option<Self>, StorypoleError> {
        // 1. Try .gaugeblock/config.toml [workspace]
        let gb_config = root_dir.join(".gaugeblock").join("config.toml");
        if gb_config.is_file()
            && let Ok(content) = fs::read_to_string(&gb_config)
        {
            #[derive(Deserialize)]
            struct Unified {
                workspace: Option<WorkspaceConfig>,
            }
            if let Ok(u) = toml::from_str::<Unified>(&content)
                && let Some(ws) = u.workspace
            {
                return Ok(Some(ws));
            }
        }

        // 2. Try .storypole/workspace.toml
        let ws_file = root_dir.join(".storypole").join("workspace.toml");
        if ws_file.is_file()
            && let Ok(content) = fs::read_to_string(&ws_file)
        {
            let ws: Self = toml::from_str(&content)?;
            return Ok(Some(ws));
        }

        // 3. Try .storypole/config.toml [workspace]
        let sp_config = root_dir.join(".storypole").join("config.toml");
        if sp_config.is_file()
            && let Ok(content) = fs::read_to_string(&sp_config)
        {
            #[derive(Deserialize)]
            struct UnifiedSp {
                workspace: Option<WorkspaceConfig>,
            }
            if let Ok(u) = toml::from_str::<UnifiedSp>(&content)
                && let Some(ws) = u.workspace
            {
                return Ok(Some(ws));
            }
        }

        Ok(None)
    }

    /// Save workspace configuration to `.storypole/workspace.toml`.
    pub fn save_to_file(&self, path: &Path) -> Result<(), StorypoleError> {
        let content =
            toml::to_string_pretty(self).map_err(|e| StorypoleError::ConfigError(e.to_string()))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| StorypoleError::IoWithPath {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        fs::write(path, content).map_err(|e| StorypoleError::IoWithPath {
            path: path.to_path_buf(),
            source: e,
        })?;
        Ok(())
    }
}
