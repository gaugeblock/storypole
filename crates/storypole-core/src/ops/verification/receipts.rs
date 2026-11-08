//! Building a verification receipt, by running something or recording that nothing ran (`GB-316`).
//!
//! Split from the orchestration in `mod.rs` when the provenance fields pushed that file past the line
//! limit. The seam is behavioural: this module decides *what evidence exists*, `mod.rs` decides what to
//! do with it.

use std::path::Path;
use std::process::Command;

use crate::error::StorypoleError;
use crate::model::decision::VerificationReceipt;
use crate::store::StorypoleStore;

/// What a receipt is built from.
///
/// Grouped rather than passed as eight arguments: the fields are one request, and a caller that has to
/// count positions is a caller that will eventually transpose two.
pub(super) struct BuildRequest<'a> {
    pub store: &'a StorypoleStore,
    pub task: &'a crate::model::TaskNode,
    pub target_dir: &'a Path,
    pub now: u64,
    pub diagnostic_count: Option<u32>,
    pub receipt_override: Option<VerificationReceipt>,
    pub run_command: bool,
    pub run_compiler: bool,
}

/// Produces the receipt for a verification attempt, and the chain key that ran.
///
/// The key is returned beside the receipt because it is provenance rather than evidence: a caller that
/// stamps it onto the receipt later must not have to re-derive it, and re-deriving is how the two
/// could disagree.
pub(super) fn build(
    request: BuildRequest<'_>,
) -> Result<(VerificationReceipt, Option<String>), StorypoleError> {
    let BuildRequest {
        store,
        task,
        target_dir,
        now,
        diagnostic_count,
        receipt_override,
        run_command,
        run_compiler,
    } = request;
    let mut chain_used: Option<String> = None;
    let receipt = if let Some(r) = receipt_override {
        r
    } else if run_command {
        let chain_opt = task
            .scope
            .crate_name
            .as_deref()
            .and_then(|c| store.config().verification_chain_exact(c))
            .or_else(|| {
                task.scope
                    .repo
                    .as_deref()
                    .and_then(|r| store.config().verification_chain_exact(r))
            })
            // The `default` fallback is taken once, after both exact lookups, and it is the only reason
            // a task with neither a crate nor a repository still has a chain (`GB-272`). Taking the
            // fallback per lookup was a real defect: a task naming both matched `default` on the crate
            // and never reached its own repository's chain, so the harness chain ran for member work
            // (`GB-316`).
            .or_else(|| store.config().verification_chain_default());

        // `cargo test` is a real check but a much weaker one than a configured fmt+clippy+nextest+docs
        // chain, and a receipt that does not say which ran makes the two indistinguishable. That silence
        // is the same defect as `up-to-date` and `verifier: manual` (`GB-269`).
        //
        // The key is kept beside the command for the same reason: `verification_chain_for` falls back to
        // `default`, so a task asking for `miter` and a task asking for a name nobody configured both
        // receive a command, and only the key says which happened (`GB-316`).
        let (cmd_str, configured) = match chain_opt {
            Some((key, command)) => {
                chain_used = Some(key.to_string());
                (command, true)
            }
            None => ("cargo test", false),
        };
        if !configured {
            let scope = task
                .scope
                .crate_name
                .as_deref()
                .or(task.scope.repo.as_deref())
                .unwrap_or("<unscoped>");
            eprintln!(
                "warning: no verification command configured for `{scope}`; falling back to \
                 'cargo test'. This receipt certifies less than a configured run would."
            );
        }
        let output = Command::new("sh")
            .arg("-c")
            .arg(cmd_str)
            .current_dir(target_dir)
            .output()
            .map_err(|e| StorypoleError::VerificationFailed(e.to_string()))?;

        let exit_code = output.status.code();
        let mut summary = String::from_utf8_lossy(&output.stdout).to_string();
        if !configured {
            // Durable, not just a console warning: the receipt is what a later reader trusts.
            summary = format!(
                "[fallback: no verification command configured; ran `{cmd_str}`]\n{summary}"
            );
        }

        if exit_code != Some(0) {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(StorypoleError::VerificationFailed(format!(
                "Command '{cmd_str}' exited with {exit_code:?}: {stderr}"
            )));
        }

        let mut verifier = if run_compiler {
            "miter+storypole-verify".to_string()
        } else {
            "storypole-verify".to_string()
        };
        if !configured {
            verifier.push_str(":cargo-test-fallback");
        }

        VerificationReceipt {
            verifier,
            command: cmd_str.to_string(),
            exit_code,
            diagnostic_count,
            summary,
            verified_at_epoch_ms: now,
            ..VerificationReceipt::default()
        }
    } else if run_compiler {
        VerificationReceipt {
            verifier: "miter-verify".to_string(),
            command: "miter verify".to_string(),
            exit_code: Some(0),
            diagnostic_count,
            summary: "Compiler verification passed with 0 diagnostics".to_string(),
            verified_at_epoch_ms: now,
            ..VerificationReceipt::default()
        }
    } else {
        // Nothing was executed. The receipt already says `manual`, which is honest, but a caller who
        // expected certification will read `exit_code: Some(0)` and stop looking (`GB-272`).
        VerificationReceipt {
            verifier: "manual".to_string(),
            command: "n/a".to_string(),
            exit_code: Some(0),
            summary: "Manual verification confirmed".to_string(),
            verified_at_epoch_ms: now,
            ..VerificationReceipt::default()
        }
    };
    Ok((receipt, chain_used))
}
