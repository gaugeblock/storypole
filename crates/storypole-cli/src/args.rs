//! Command-line argument definitions and Clap parsers for `storypole-cli`.

use crate::plan_args::PlanArgs;
use crate::update_args::UpdateArgs;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

// Task lifecycle, deferral and decision arguments live in `task_args` to hold this file under the
// hard 250-line ceiling; re-exported here so `crate::args::*` still resolves every name.
pub use crate::task_args::{
    AskDecisionArgs, DecideArgs, DeferArgs, HandoffArgs, SplitArgs, VerifyArgs,
};

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
    /// Audit every ledger for corruption and replay truth
    LedgerVerify,
    /// Compare the unified index against the ledger replay
    LedgerReconcile,
    /// Report the key remint a store would need to mint under its own prefix (GB-347)
    Remint {
        /// Apply the remint. Refused for now: it must rewrite references in the same pass.
        #[arg(long)]
        apply: bool,
    },
    /// Inspect the repository configuration
    Config(ConfigArgs),
    List(ListArgs),
    /// Show full context briefing for a task
    Show {
        identifier: String,
    },
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
    /// Return tasks to the backlog when their claim lease has expired
    Reap {
        /// Report what would be reaped without changing the ledger
        #[arg(long)]
        dry_run: bool,
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
    /// Archive a completed or closed task out of active backlog views
    Archive {
        identifier: String,
    },
    /// Restore an archived task back to active backlog
    Unarchive {
        identifier: String,
    },
    /// Manage multi-repository workspace members and distributed migration
    Workspace(crate::workspace_args::WorkspaceArgs),
    /// Inspect or repair the local telemetry spool outbox
    /// Split one store's tickets into the repositories that own them
    Migrate {
        /// Report what would move without writing anything
        #[arg(long)]
        dry_run: bool,
        /// Move only the tickets whose own scope names their repository, leaving adoptions for review
        #[arg(long)]
        scope_only: bool,
    },
    Spool(SpoolArgs),
    /// Start the stdio JSON-RPC 2.0 Model Context Protocol server
    Mcp,
}

/// Arguments for the `spool` command group.
#[derive(Debug, Args)]
pub struct SpoolArgs {
    #[command(subcommand)]
    pub command: SpoolCommands,
}

/// Subcommands that operate on the append-only telemetry outbox.
#[derive(Debug, Subcommand)]
pub enum SpoolCommands {
    /// Rewrite legacy bare-trace lines into canonical `SpoolEntry` envelopes
    ///
    /// Idempotent: lines already carrying an envelope are left byte-for-byte untouched, so a second
    /// run reports zero rewrites.
    Migrate {
        /// Outbox path (defaults to `<.gaugeblock>/spool/outbox.jsonl`)
        #[arg(long)]
        path: Option<PathBuf>,
        /// Report what would be rewritten without touching the file
        #[arg(long)]
        dry_run: bool,
    },
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
    #[arg(long, default_value = "feature", value_parser = crate::task_args::valid_kind)]
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
    #[arg(long, value_parser = crate::task_args::valid_kind)]
    pub kind: Option<String>,
    #[arg(long)]
    pub label: Option<String>,
    #[arg(long)]
    pub priority: Option<u8>,
    #[arg(long)]
    pub include_archived: bool,
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

/// Arguments for `storypole config`.
#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

/// What to do with the configuration.
#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Report settings that are present but wrong (`GB-273`)
    Check(ConfigCheckArgs),
}

/// Arguments for `storypole config check`.
#[derive(Debug, Args)]
pub struct ConfigCheckArgs {
    /// Audit every repository in the workspace, discovered through `WorkspaceTopology`.
    ///
    /// Membership comes from the topology, never a glob - the same rule as `benchwork tidy --workspace`
    /// (`GB-263`). A repository with no configuration is skipped rather than reported, because not being
    /// adopted yet is not a defect.
    #[arg(long, default_value_t = false)]
    pub workspace: bool,
}
