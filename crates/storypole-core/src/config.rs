//! Storypole repository configuration loader and validator.

use crate::error::StorypoleError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Per-repository or meta-harness configuration loaded from `.storypole/config.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorypoleConfig {
    /// Human-readable ticket prefix (e.g., "GB", "SP", "MTR"). Generates IDs like `SP-14`.
    pub ticket_prefix: String,
    /// Optional allow-list of valid repository names (enforced when non-empty).
    pub allowed_repos: Vec<String>,
    /// Optional allow-list of valid crate/package names (enforced when non-empty).
    pub allowed_crates: Vec<String>,
    /// Default lease time-to-live in minutes before an `InProgress` agent claim expires.
    pub claim_ttl_minutes: u32,
    /// Optional map of `(crate_or_repo_name, verification_command)`.
    pub verification_commands: Vec<(String, String)>,
}

impl Default for StorypoleConfig {
    fn default() -> Self {
        Self {
            ticket_prefix: "SP".to_string(),
            allowed_repos: Vec::new(),
            allowed_crates: Vec::new(),
            claim_ttl_minutes: 60,
            verification_commands: Vec::new(),
        }
    }
}

impl StorypoleConfig {
    /// Load configuration from a `.storypole/config.toml` file.
    pub fn load_from_file(path: &Path) -> Result<Self, StorypoleError> {
        let content = fs::read_to_string(path).map_err(|e| StorypoleError::IoWithPath {
            path: path.to_path_buf(),
            source: e,
        })?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    /// Load configuration from `.gaugeblock/config.toml` (if `[storypole]` table present)
    /// or fallback to legacy path.
    pub fn load_unified_or_legacy(
        root_dir: &Path,
        legacy_path: &Path,
    ) -> Result<Self, StorypoleError> {
        let unified_path = root_dir.join(".gaugeblock").join("config.toml");
        if unified_path.is_file()
            && let Ok(content) = fs::read_to_string(&unified_path)
        {
            #[derive(Deserialize)]
            struct Unified {
                storypole: Option<StorypoleConfig>,
            }
            if let Ok(u) = toml::from_str::<Unified>(&content)
                && let Some(c) = u.storypole
            {
                return Ok(c);
            }
        }
        Self::load_from_file(legacy_path)
    }

    /// Save configuration to `.storypole/config.toml`.
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

    /// Resolve configured verification command for a crate or repo name.
    pub fn verification_command_for(&self, target: &str) -> Option<&str> {
        self.verification_commands
            .iter()
            .find(|(k, _)| k == target)
            .map(|(_, cmd)| cmd.as_str())
    }

    /// Validate whether the given repo and crate are permitted by this configuration.
    pub fn validate_scope(
        &self,
        repo: Option<&str>,
        crate_name: Option<&str>,
    ) -> Result<(), StorypoleError> {
        if let Some(r) = repo
            && !self.allowed_repos.is_empty()
            && !self.allowed_repos.iter().any(|x| x == r)
        {
            return Err(StorypoleError::ScopeViolation(format!(
                "Repository '{r}' not in allowed_repos: {:?}",
                self.allowed_repos
            )));
        }
        if let Some(c) = crate_name
            && !self.allowed_crates.is_empty()
            && !self.allowed_crates.iter().any(|x| x == c)
        {
            return Err(StorypoleError::ScopeViolation(format!(
                "Crate '{c}' not in allowed_crates: {:?}",
                self.allowed_crates
            )));
        }
        Ok(())
    }
}
