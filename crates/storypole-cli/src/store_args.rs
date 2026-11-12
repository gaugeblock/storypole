//! Arguments for the store-level maintenance verbs: remint, relink, migrate and spool.
//!
//! Extracted from `args` when the command enum crossed the 250-line ceiling (`GB-347`), following the same rule as
//! `task_args`: the flag set of one family lives beside itself, and `crate::args::*` still resolves every name.

use std::path::PathBuf;

use clap::{Args, Subcommand};

/// `storypole remint` - report or apply a store's key remint.
#[derive(Debug, Args)]
pub struct RemintArgs {
    /// Apply the remint, rewriting every key, edge target, citation and plan path it moves.
    #[arg(long)]
    pub apply: bool,
}

/// `storypole relink` - repair cross-store references and link harness tickets to the work.
#[derive(Debug, Args)]
pub struct RelinkArgs {
    /// Apply the repair and write the links
    #[arg(long)]
    pub apply: bool,
}

/// `storypole migrate` - split one store's tickets into the repositories that own them.
#[derive(Debug, Args)]
pub struct MigrateArgs {
    /// Report what would move without writing anything
    #[arg(long)]
    pub dry_run: bool,
    /// Move only the tickets whose own scope names their repository, leaving adoptions for review
    #[arg(long)]
    pub scope_only: bool,
}

/// `storypole refresh-projections` - rewrite ticket blocks that disagree with their own events (`GB-349`).
#[derive(Debug, Args)]
pub struct RefreshProjectionsArgs {
    /// Rewrite the records that disagree; without this the pass only reports them
    #[arg(long)]
    pub apply: bool,
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
