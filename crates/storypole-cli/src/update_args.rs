//! Update command argument definitions.

use clap::Args;

/// Arguments for updating an existing task.
#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Task ticket key, slug, or UUID
    pub identifier: String,
    /// New title for the task
    #[arg(long)]
    pub title: Option<String>,
    /// New description for the task
    #[arg(long)]
    pub desc: Option<String>,
    /// New priority (1-5)
    #[arg(long)]
    pub priority: Option<u8>,
    /// Target repository
    #[arg(long)]
    pub repo: Option<String>,
    /// Target crate name
    #[arg(long)]
    pub crate_name: Option<String>,
    /// Target module path
    #[arg(long)]
    pub module: Option<String>,
    /// Target file paths
    #[arg(long = "file", value_delimiter = ',')]
    pub files: Vec<String>,
    /// Target code symbols
    #[arg(long = "symbol", value_delimiter = ',')]
    pub symbols: Vec<String>,
    #[arg(long = "label", value_delimiter = ',')]
    /// Labels/tags
    pub labels: Vec<String>,
    /// Concrete acceptance criteria
    #[arg(long = "criterion", value_delimiter = ',')]
    pub criteria: Vec<String>,
}
