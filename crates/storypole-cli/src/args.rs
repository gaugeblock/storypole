//! Command-line argument definitions and Clap parsers for `storypole-cli`.

use crate::plan_args::PlanArgs;
use crate::update_args::UpdateArgs;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// Git-native backlog, task DAG, decision lifecycle engine, and MCP server.
#[derive(Debug, Parser)]
#[command(name = "storypole", version, about)]
pub struct Cli {
    /// Format output as raw JSON for machine consumption
    #[arg(long, global = true)]
    pub json: bool,

    /// Target repository directory containing `.storypole` (defaults to current dir)
    #[arg(long, global = true)]
    pub dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Initialize a new .storypole repository
    Init(InitArgs),
    /// Add a new backlog task node
    Add(AddArgs),
    /// List backlog tasks matching filters
    List(ListArgs),
    /// Show full context briefing for a task
    Show { identifier: String },
    /// Find next ready task with context briefing and collision warnings
    Next(NextArgs),
    /// Acquire an active agent claim lease on a task
    Claim {
        identifier: String,
        agent_id: String,
        #[arg(long)]
        branch: Option<String>,
    },
    /// Renew claim lease for an active InProgress task
    Heartbeat {
        identifier: String,
        agent_id: String,
    },
    /// Record structured handoff packet and release lease
    Handoff(HandoffArgs),
    /// Defer an out-of-band issue or tech-debt in <5ms
    Defer(DeferArgs),
    /// Connect two tasks with a directed edge (BlockedBy, RelatedTo, ParentEpic)
    Link {
        source: String,
        target: String,
        #[arg(long, default_value = "blocked_by")]
        kind: String,
    },
    /// Split an oversized task into an Epic and child subtasks
    Split(SplitArgs),
    /// Raise an architectural decision requirement, blocking task
    AskDecision(AskDecisionArgs),
    /// Resolve a pending decision with chosen option and rationale
    Decide(DecideArgs),
    /// Supersede a previously resolved decision
    SupersedeDecision {
        identifier: String,
        decision_id: String,
        reason: String,
    },
    /// Verify a task and certify with a VerificationReceipt
    Verify(VerifyArgs),
    /// Update an existing backlog task node
    Update(UpdateArgs),
    /// Manage attached execution plans and design markdown blueprints
    Plan(PlanArgs),
    /// Display ASCII or Mermaid DAG graph
    Graph {
        #[arg(long, default_value = "ascii")]
        format: String,
    },
    /// Start the stdio JSON-RPC 2.0 Model Context Protocol server
    Mcp,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(long, default_value = "SP")]
    pub prefix: String,
    #[arg(long, default_value_t = 60)]
    pub claim_ttl: u32,
}

#[derive(Debug, Args)]
pub struct AddArgs {
    pub title: String,
    #[arg(long)]
    pub desc: Option<String>,
    #[arg(long, default_value = "feature")]
    pub kind: String,
    #[arg(long, default_value_t = 3)]
    pub priority: u8,
    #[arg(long)]
    pub repo: Option<String>,
    #[arg(long)]
    pub crate_name: Option<String>,
    #[arg(long)]
    pub module: Option<String>,
    #[arg(long)]
    pub files: Vec<String>,
    #[arg(long)]
    pub symbols: Vec<String>,
    #[arg(long)]
    pub labels: Vec<String>,
    #[arg(long)]
    pub criteria: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(long)]
    pub repo: Option<String>,
    #[arg(long)]
    pub crate_name: Option<String>,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub kind: Option<String>,
    #[arg(long)]
    pub label: Option<String>,
    #[arg(long)]
    pub priority: Option<u8>,
}

#[derive(Debug, Args)]
pub struct NextArgs {
    #[arg(long)]
    pub repo: Option<String>,
    #[arg(long)]
    pub crate_name: Option<String>,
    #[arg(long)]
    pub label: Option<String>,
    #[arg(long)]
    pub priority: Option<u8>,
    #[arg(long)]
    pub exclude_file_collisions: bool,
    #[arg(long)]
    pub claim: Option<String>,
}

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
