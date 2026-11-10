//! `storypole config` — inspection of the repository configuration (`GB-273`).

use crate::args::ConfigCommand;
use crate::json_out::print_json;
use anyhow::Result;
use std::path::Path;
use storypole_core::StorypoleStore;
use storypole_core::config::StorypoleConfig;
use storypole_core::config_audit::{ConfigFinding, ConfigSeverity};

/// Runs a `config` subcommand against the loaded store.
///
/// Exit code is zero for warnings and non-zero only when a finding is an error: a warning means the
/// configuration works but claims more than it delivers, and failing a build over that would train people
/// to pass a flag rather than read the output.
///
/// # Errors
///
/// Returns an error when any finding is an error, or when the JSON envelope cannot be serialised.
pub fn handle_config(
    store: &StorypoleStore,
    root_dir: &Path,
    command: ConfigCommand,
    json: bool,
) -> Result<()> {
    match command {
        ConfigCommand::Check(args) => {
            if args.workspace {
                return check_workspace(root_dir, json);
            }
            report(&[(None, store.config().audit())], json)
        }
    }
}

/// Audits every suite member, skipping repositories that carry no configuration.
///
/// Reads through `load_unified_or_legacy`, which creates nothing: an audit must not leave index files
/// behind in repositories it merely inspected. Membership comes from `WorkspaceTopology`, never a glob.
fn check_workspace(root_dir: &Path, json: bool) -> Result<()> {
    let topology = gaugeblock_workspace::WorkspaceTopology::discover(root_dir);
    let mut audited = Vec::new();

    let mut inherited_from_suite: Vec<String> = Vec::new();

    for member in topology.members() {
        let legacy = member.path.join(".gaugeblock/storypole/config.toml");
        if let Ok(config) = StorypoleConfig::load_unified_or_legacy(&member.path, &legacy) {
            // The audit still reads the file rather than opening a store - it must not leave index files in
            // repositories it merely inspects - but it applies the same inheritance `verify` does, or it reports a
            // fallback that no longer happens (`SP-3`).
            let declared_locally = !config.verification_commands.is_empty();
            let config = config.inheriting(storypole_core::store::chains_declared_by_suite(
                &member.path,
            ));
            if !declared_locally && !config.verification_commands.is_empty() {
                inherited_from_suite.push(member.name.clone());
            }
            audited.push((Some(member.name.clone()), config.audit()));
        }
    }

    // Named rather than silent: a member that passes this audit because of another repository's configuration should
    // say so, or the next reader has no way to tell which file they are looking at (`SP-3`).
    if !json {
        for name in &inherited_from_suite {
            eprintln!(
                "note: {name} declares no verification chains of its own; it uses the suite root's, as does `storypole verify`"
            );
        }
    }

    if audited.is_empty() {
        anyhow::bail!("no repository in the workspace carries a storypole configuration");
    }
    report(&audited, json)
}

/// Prints findings and fails only when one is an error.
fn report(audit: &[(Option<String>, Vec<ConfigFinding>)], json: bool) -> Result<()> {
    let errors: usize = audit
        .iter()
        .flat_map(|(_, findings)| findings)
        .filter(|finding| finding.severity == ConfigSeverity::Error)
        .count();
    let total: usize = audit.iter().map(|(_, findings)| findings.len()).sum();

    if json {
        let findings: Vec<serde_json::Value> = audit
            .iter()
            .flat_map(|(repo, findings)| {
                findings.iter().map(move |finding| {
                    serde_json::json!({
                        "repo": repo,
                        "field": finding.field,
                        "message": finding.message,
                        "severity": if finding.severity == ConfigSeverity::Error { "error" } else { "warning" },
                    })
                })
            })
            .collect();
        let payload = serde_json::json!({
            "errors": errors,
            "findings": findings,
            "repositories": audit.len(),
        });
        print_json("config-check", &payload)?;
    } else {
        for (repo, findings) in audit {
            for finding in findings {
                let label = match finding.severity {
                    ConfigSeverity::Error => "error",
                    ConfigSeverity::Warning => "warning",
                };
                match repo {
                    Some(name) => {
                        println!("{name}: {label}: {} {}", finding.field, finding.message);
                    }
                    None => println!("{label}: {} {}", finding.field, finding.message),
                }
            }
        }
        println!();
        println!(
            "{} repositories audited, {total} finding(s), {errors} error(s).",
            audit.len()
        );
    }

    if errors > 0 {
        anyhow::bail!("{errors} configuration error(s)");
    }
    Ok(())
}
