//! Configuration audit: report settings that are present but wrong (`GB-273`).
//!
//! A manual sweep in 2026-11 found `verification_commands` empty in all eleven configs, `allowed_repos` in
//! two different states across ten repositories, and `allowed_crates` empty everywhere and unenforced.
//! None of it was detectable without reading every file by hand, because **nothing validates a config
//! once it loads** - a field that is present but empty is indistinguishable from one deliberately cleared.
//!
//! Findings are warnings unless they break operation. The point is not to make a repository fail to build;
//! it is to make "present but empty" stop being invisible.

use crate::config::StorypoleConfig;

/// How much a finding matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]

pub enum ConfigSeverity {
    /// The configuration still works, but does less than it appears to.
    Warning,
    /// The configuration will not behave as configured.
    Error,
}

/// One problem found in a configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigFinding {
    /// The setting concerned.
    pub field: &'static str,
    /// What is wrong and what would fix it.
    pub message: String,
    /// Whether this breaks operation or only weakens it.
    pub severity: ConfigSeverity,
}

impl StorypoleConfig {
    /// Reports settings that are present but wrong.
    ///
    /// Deliberately does not report an unset `allowed_repos` as a problem: empty means unrestricted, which
    /// is legitimate for a meta-harness root. It reports the things that make a config *claim* more than it
    /// delivers.
    #[must_use]
    pub fn audit(&self) -> Vec<ConfigFinding> {
        let mut findings = Vec::new();

        if self.ticket_prefix.trim().is_empty() {
            findings.push(ConfigFinding {
                field: "ticket_prefix",
                message: "is empty, so new tasks cannot be given a key".to_string(),
                severity: ConfigSeverity::Error,
            });
        } else if !self
            .ticket_prefix
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            findings.push(ConfigFinding {
                field: "ticket_prefix",
                message: format!(
                    "`{}` is not upper-case alphanumeric, which makes keys awkward to type and sort",
                    self.ticket_prefix
                ),
                severity: ConfigSeverity::Warning,
            });
        }

        if self.verification_commands.is_empty() {
            findings.push(ConfigFinding {
                field: "verification_commands",
                message:
                    "is empty, so `storypole verify` falls back to `cargo test` - a receipt then \
                          certifies less than it appears to"
                        .to_string(),
                severity: ConfigSeverity::Warning,
            });
        }

        for (key, command) in &self.verification_commands {
            if command.trim().is_empty() {
                findings.push(ConfigFinding {
                    field: "verification_commands",
                    message: format!(
                        "entry `{key}` has an empty command, so verify would succeed by doing nothing"
                    ),
                    severity: ConfigSeverity::Error,
                });
            }
        }

        let mut seen: Vec<&str> = Vec::new();
        for (key, _) in &self.verification_commands {
            if seen.contains(&key.as_str()) {
                findings.push(ConfigFinding {
                    field: "verification_commands",
                    message: format!(
                        "entry `{key}` appears more than once, so all but the first are dead"
                    ),
                    severity: ConfigSeverity::Warning,
                });
            }
            seen.push(key.as_str());
        }

        if self.claim_ttl_minutes == 0 {
            findings.push(ConfigFinding {
                field: "claim_ttl_minutes",
                message: "is 0, so a claim never expires and `storypole reap` can never reclaim it"
                    .to_string(),
                severity: ConfigSeverity::Warning,
            });
        }

        for repo in &self.allowed_repos {
            if repo.contains('/') || repo.contains('\\') {
                findings.push(ConfigFinding {
                    field: "allowed_repos",
                    message: format!(
                        "`{repo}` looks like a path; entries are compared against repository names"
                    ),
                    severity: ConfigSeverity::Warning,
                });
            }
        }

        findings
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn config() -> StorypoleConfig {
        StorypoleConfig {
            ticket_prefix: "GB".to_string(),
            allowed_repos: Vec::new(),
            allowed_crates: Vec::new(),
            claim_ttl_minutes: 60,
            verification_commands: vec![(
                "gaugeblock".to_string(),
                "cargo nextest run".to_string(),
            )],
        }
    }

    #[test]
    fn a_healthy_config_reports_nothing() {
        assert!(config().audit().is_empty());
    }

    #[test]
    fn empty_verification_commands_is_reported_as_a_weaker_gate() {
        let mut cfg = config();
        cfg.verification_commands.clear();
        let findings = cfg.audit();

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "verification_commands");
        assert_eq!(findings[0].severity, ConfigSeverity::Warning);
        assert!(findings[0].message.contains("cargo test"));
    }

    #[test]
    fn an_empty_command_is_an_error_because_verify_would_pass_by_doing_nothing() {
        let mut cfg = config();
        cfg.verification_commands = vec![("gaugeblock".to_string(), String::new())];

        assert_eq!(cfg.audit()[0].severity, ConfigSeverity::Error);
    }

    #[test]
    fn duplicate_keys_are_reported_because_all_but_the_first_are_dead() {
        let mut cfg = config();
        cfg.verification_commands = vec![
            ("gaugeblock".to_string(), "a".to_string()),
            ("gaugeblock".to_string(), "b".to_string()),
        ];

        assert!(
            cfg.audit()
                .iter()
                .any(|finding| finding.message.contains("more than once"))
        );
    }

    #[test]
    fn an_empty_prefix_is_an_error_and_a_messy_one_is_a_warning() {
        let mut cfg = config();
        cfg.ticket_prefix = String::new();
        assert_eq!(cfg.audit()[0].severity, ConfigSeverity::Error);

        cfg.ticket_prefix = "gb-lower".to_string();
        assert_eq!(cfg.audit()[0].severity, ConfigSeverity::Warning);
    }

    #[test]
    fn a_path_in_allowed_repos_is_reported() {
        let mut cfg = config();
        cfg.allowed_repos = vec!["/Users/me/repos/miter".to_string()];

        assert!(
            cfg.audit()
                .iter()
                .any(|finding| finding.field == "allowed_repos")
        );
    }
}
