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
    /// New description for the task, replacing whatever is there
    #[arg(long, conflicts_with_all = ["append_desc", "append_desc_file"])]
    pub desc: Option<String>,
    /// Append a paragraph to the task's description instead of replacing it
    #[arg(long)]
    pub append_desc: Option<String>,
    /// Append to the description from a file, or `-` for stdin
    ///
    /// The shell never touches the content, which matters for the same reason as `--criterion-file`: a
    /// description is prose, and three silent corruptions in one session came through inline arguments.
    #[arg(long)]
    pub append_desc_file: Option<String>,
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
    /// Concrete acceptance criteria. Repeatable; commas are NOT a separator, because criteria are prose
    /// and splitting them silently turns one into several (`GB-250`).
    #[arg(long = "criterion")]
    pub criteria: Vec<String>,
    /// Read one acceptance criterion from a file, or `-` for stdin.
    ///
    /// The shell never touches the content, so no quoting or escaping rule can mangle it. Three silent
    /// corruptions in one session came through inline arguments: backticks executed by zsh, quotes eaten
    /// by a multi-flag commit, and comma-splitting by the delimiter that used to be on `--criterion`.
    #[arg(long = "criterion-file")]
    pub criterion_file: Option<String>,
}
