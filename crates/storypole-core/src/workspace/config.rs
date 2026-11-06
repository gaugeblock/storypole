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
    ///
    /// Only the unified read goes through `gaugeblock-workspace` (`GB-189`). `SuiteConfig` owns the
    /// lookup and the parse of `.gaugeblock/config.toml`, which is the file every tool shares; the two
    /// legacy paths below are storypole's own files - `.storypole/workspace.toml` is a bare table and
    /// `.storypole/config.toml` is a per-tool document - so reaching `SuiteConfig` for them would model
    /// one tool's history as suite policy. They stay, with their order intact, because a checkout that
    /// predates the unified file still has to load.
    pub fn load_from_root(root_dir: &Path) -> Result<Option<Self>, StorypoleError> {
        // 1. Try .gaugeblock/config.toml [workspace]
        if let Ok(suite) = gaugeblock_workspace::SuiteConfig::load(root_dir)
            && let Some(section) = suite.section("workspace")
            && let Ok(ws) = section.clone().try_into::<Self>()
        {
            return Ok(Some(ws));
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
