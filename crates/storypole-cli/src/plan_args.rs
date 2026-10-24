//! Plan command line argument definitions.

use clap::{Args, Subcommand};
use std::path::PathBuf;

/// Manage attached execution plans and design markdown blueprints.
#[derive(Debug, Args)]
pub struct PlanArgs {
    #[command(subcommand)]
    pub command: PlanCommands,
}

#[derive(Debug, Subcommand)]
pub enum PlanCommands {
    /// Attach or update an execution plan on a task
    Set {
        /// Task ticket key, slug, or UUID
        identifier: String,
        /// Path to Markdown design plan file
        #[arg(long)]
        file: PathBuf,
        /// Optional high-level plan summary (defaults to first line of file)
        #[arg(long)]
        summary: Option<String>,
        /// Plan steps in the format "<step_id>|<title>|<verification_gate>"
        #[arg(long = "step")]
        steps: Vec<String>,
    },
    /// Display the attached plan for a task
    Show {
        /// Task ticket key, slug, or UUID
        identifier: String,
    },
}
