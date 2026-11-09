//! Clap argument structs for task lifecycle, deferral and decision commands.
//!
//! Split out of `args.rs` to hold that file under the hard 250-line ceiling (`GB-182`). Re-exported
//! from `crate::args`, so every existing import path keeps working.

use clap::Args;
use storypole_core::model::task::TaskKind;

#[derive(Debug, Args)]
pub struct HandoffArgs {
    pub identifier: String,
    pub agent_id: String,
    #[arg(long)]
    pub completed: Vec<String>,
    #[arg(long)]
    pub remaining: Vec<String>,
    #[arg(long)]
    pub blocker: Option<String>,
    #[arg(long)]
    pub git_ref: Option<String>,
    #[arg(long)]
    pub dirty_files: Vec<String>,
    #[arg(long, default_value = "ready")]
    pub status: String,
}

#[derive(Debug, Args)]
pub struct DeferArgs {
    pub title: String,
    #[arg(long)]
    pub desc: Option<String>,
    #[arg(long, default_value = "bug", value_parser = valid_kind)]
    pub kind: String,
    #[arg(long)]
    pub from: Option<String>,
    #[arg(long, default_value = "user")]
    pub captured_by: String,
    #[arg(long)]
    pub repo: Option<String>,
    #[arg(long)]
    pub crate_name: Option<String>,
    #[arg(long)]
    pub files: Vec<String>,
    #[arg(long)]
    pub symbols: Vec<String>,
    #[arg(long)]
    pub labels: Vec<String>,
}

#[derive(Debug, Args)]
pub struct SplitArgs {
    pub identifier: String,
    pub child_titles: Vec<String>,
    #[arg(long, default_value_t = true)]
    pub sequential: bool,
}

#[derive(Debug, Args)]
pub struct AskDecisionArgs {
    pub identifier: String,
    pub question: String,
    #[arg(long, default_value = "general")]
    pub category: String,
    #[arg(long = "option")]
    pub options: Vec<String>,
    #[arg(long)]
    pub parent_id: Option<String>,
}

#[derive(Debug, Args)]
pub struct DecideArgs {
    pub identifier: String,
    pub decision_id: String,
    #[arg(long)]
    pub choose: String,
    #[arg(long)]
    pub rationale: String,
    #[arg(long, default_value = "user")]
    pub resolved_by: String,
    #[arg(long)]
    pub criterion: Option<String>,
}

#[derive(Debug, Args)]
pub struct VerifyArgs {
    pub identifier: String,
    /// Accepted for compatibility; execution is now the default.
    #[arg(long)]
    pub run: bool,
    #[arg(long)]
    pub compiler: bool,
    /// Record a human attestation instead of executing the configured gate.
    ///
    /// The default is to execute, because a command whose purpose is certification should not be able to
    /// return `exit_code: 0` having run nothing - that was `GB-272`, and it produced receipts that looked
    /// like proof while being a statement of intent.
    #[arg(long)]
    pub manual: bool,
    #[arg(long)]
    pub verifier: Option<String>,
    #[arg(long)]
    pub summary: Option<String>,
}

/// Kinds a task can be, derived from the contract so the two cannot drift (`GB-343`).
pub(crate) fn kind_names() -> Vec<&'static str> {
    TaskKind::ALL.into_iter().map(TaskKind::as_str).collect()
}

/// Validates a `--kind` value, for clap.
///
/// Unknown values used to become `Feature` silently, so `--kind perf` created a Feature and said
/// nothing (`GB-196`). A misspelled kind is a mistake to report, not to reinterpret.
pub(crate) fn valid_kind(value: &str) -> Result<String, String> {
    match TaskKind::from_wire(value) {
        Some(kind) => Ok(kind.as_str().to_string()),
        None => Err(format!(
            "unknown kind `{value}`; expected one of: {}",
            kind_names().join(", ")
        )),
    }
}

#[cfg(test)]
mod kind_tests {
    #![allow(clippy::unwrap_used)]

    use super::valid_kind;

    #[test]
    fn docs_is_an_accepted_kind() {
        // GB-342: documentation work had no kind that described it, so it was filed as chore or feature.
        assert_eq!(valid_kind("docs").unwrap(), "docs");
    }

    #[test]
    fn a_kind_is_case_insensitive() {
        assert_eq!(valid_kind("DOCS").unwrap(), "docs");
    }

    #[test]
    fn every_kind_the_contract_declares_is_accepted() {
        // The drift this replaces: the CLI kept its own array, so a kind could exist in the contract and be
        // rejected by the tool that creates tasks (`GB-343`).
        for kind in storypole_core::model::task::TaskKind::ALL {
            assert_eq!(valid_kind(kind.as_str()).unwrap(), kind.as_str());
        }
    }

    #[test]
    fn an_unknown_kind_lists_the_accepted_kinds() {
        // The probe must stay a value no kind claims: perf was unknown until GB-343 and is now valid,
        // which is how this test failed the moment the kinds were added.
        let message = valid_kind("nonsense").unwrap_err();
        assert!(
            message.contains("docs"),
            "the error names what is accepted: {message}"
        );
        assert!(
            message.contains("build"),
            "including kinds added after this test was written: {message}"
        );
        assert!(
            message.contains("unknown kind"),
            "and says what was wrong: {message}"
        );
    }
}
