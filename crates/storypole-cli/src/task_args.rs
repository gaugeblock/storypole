//! Clap argument structs for task lifecycle, deferral and decision commands.
//!
//! Split out of `args.rs` to hold that file under the hard 250-line ceiling (`GB-182`). Re-exported
//! from `crate::args`, so every existing import path keeps working.

use clap::Args;

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
    #[arg(long, default_value = "bug")]
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
    #[arg(long)]
    pub run: bool,
    #[arg(long)]
    pub compiler: bool,
    #[arg(long)]
    pub verifier: Option<String>,
    #[arg(long)]
    pub summary: Option<String>,
}
