//! `storypole config` — inspection of the repository configuration.

use crate::args::ConfigCommand;
use crate::json_out::print_json;
use anyhow::Result;
use storypole_core::StorypoleStore;
use storypole_core::config_audit::ConfigSeverity;

/// Runs a `config` subcommand against the loaded store.
///
/// Exit code is non-zero only when a finding is an error: a warning means the configuration works but
/// claims more than it delivers, and failing a build over that would train people to pass a flag rather
/// than read the output (`GB-273`).
///
/// # Errors
///
/// Returns an error when a finding is an error, or when the JSON envelope cannot be serialised.
pub fn handle_config(store: &StorypoleStore, command: ConfigCommand, json: bool) -> Result<()> {
    match command {
        ConfigCommand::Check => {
            let findings = store.config().audit();
            let errors = findings
                .iter()
                .filter(|finding| finding.severity == ConfigSeverity::Error)
                .count();

            if json {
                let payload = serde_json::json!({
                    "errors": errors,
                    "findings": findings
                        .iter()
                        .map(|finding| serde_json::json!({
                            "field": finding.field,
                            "message": finding.message,
                            "severity": if finding.severity == ConfigSeverity::Error { "error" } else { "warning" },
                        }))
                        .collect::<Vec<_>>(),
                });
                print_json("config-check", &payload)?;
            } else if findings.is_empty() {
                println!("Configuration looks sound: no findings.");
            } else {
                for finding in &findings {
                    let label = match finding.severity {
                        ConfigSeverity::Error => "error",
                        ConfigSeverity::Warning => "warning",
                    };
                    println!("{label}: {} {}", finding.field, finding.message);
                }
                println!();
                println!("{} finding(s), {errors} error(s).", findings.len());
            }

            if errors > 0 {
                anyhow::bail!("{errors} configuration error(s)");
            }
            Ok(())
        }
    }
}
