//! Command-line argument definitions for workspace operations.

use clap::{Args, Subcommand};

/// Arguments for `storypole workspace` subcommands.
#[derive(Debug, Args)]
pub struct WorkspaceArgs {
    #[command(subcommand)]
    pub command: WorkspaceCommands,
}

/// Subcommands under `storypole workspace`.
#[derive(Debug, Subcommand)]
pub enum WorkspaceCommands {
    /// List member repositories discovered in the workspace
    List,
    /// Initialize .storypole directories in member repositories that do not have one
    Init,
    /// Copy tasks that name a member repository from the root store into that member's store
    Distribute {
        /// Perform dry-run audit without writing changes to disk
        #[arg(long)]
        dry_run: bool,
    },
}
