//! Command-line interface and entrypoint for `storypole`.

#![deny(unsafe_code)]

mod args;
mod format;
mod format_decisions;
mod handlers;
mod json_out;
mod plan_args;
mod store_args;
mod task_args;
mod update_args;
mod workspace_args;

use anyhow::Result;
use args::{Cli, Commands, SpoolCommands};
use clap::Parser;
use handlers::*;
use std::env;
use storypole_core::store::StorypoleStore;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // One span per invocation, so the panel can answer what the suite did rather than only what inference cost
    // (`GB-339`). Opened before any work and closed with the outcome; it cannot fail the command.
    let command = std::env::args().nth(1).filter(|arg| !arg.starts_with('-'));
    let invocation = gaugeblock_workspace::InvocationSpan::start("storypole", command.as_deref());

    let filter = if cli.json { "error" } else { "warn" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .with_writer(std::io::stderr)
        .init();

    let root_dir = match cli.dir {
        Some(d) => d,
        None => env::current_dir()?,
    };

    // A closure so the result can be inspected before the span closes: with `?` at this level an error returned
    // from `main` and the span was recorded as still running (`GB-339`).
    let result = (|| -> Result<()> {
        match cli.command {
            Commands::Init(args) => handle_init(&root_dir, args, cli.json)?,
            Commands::Workspace(args) => match args.command {
                crate::workspace_args::WorkspaceCommands::List => {
                    handle_workspace_list(&root_dir, cli.json)?;
                }
                crate::workspace_args::WorkspaceCommands::Init => {
                    handle_workspace_init(&root_dir, cli.json)?;
                }
                crate::workspace_args::WorkspaceCommands::Distribute { dry_run } => {
                    handle_workspace_distribute(&root_dir, dry_run, cli.json)?;
                }
            },
            Commands::Mcp => {
                storypole_mcp::run_stdio_server(&root_dir)
                    .map_err(|e| anyhow::anyhow!("MCP server error: {e}"))?;
            }
            Commands::Config(args) => {
                // Handled before the catch-all: `--workspace` audits repositories it was not launched in, so
                // requiring an initialised store at the launch directory would refuse the one case that
                // matters - a member auditing its suite (`GB-305`).
                handle_config(&root_dir, args.command, cli.json)?
            }
            cmd => {
                let mut store = StorypoleStore::open(&root_dir)?;
                match cmd {
                    Commands::Init(_)
                    | Commands::Mcp
                    | Commands::Workspace(_)
                    | Commands::Config(_) => {
                        unreachable!()
                    }
                    Commands::Add(args) => handle_add(&mut store, args, cli.json)?,
                    Commands::LedgerVerify => handle_ledger_verify(&root_dir, cli.json)?,
                    Commands::LedgerReconcile => handle_ledger_reconcile(&root_dir, cli.json)?,
                    Commands::RefreshProjections(args) => {
                        handle_refresh_projections(&root_dir, args.apply, cli.json)?
                    }
                    Commands::Remint(args) => crate::handlers::remint_cmds::handle_remint(
                        &root_dir, args.apply, cli.json,
                    )?,
                    Commands::Relink(args) => crate::handlers::relink_cmds::handle_relink(
                        &root_dir, args.apply, cli.json,
                    )?,
                    Commands::List(args) => handle_list(&store, args, cli.json)?,
                    Commands::Show { identifier } => handle_show(&store, &identifier, cli.json)?,
                    Commands::Next(args) => handle_next(&mut store, args, cli.json)?,
                    Commands::Claim {
                        identifier,
                        agent_id,
                        branch,
                    } => handle_claim(
                        &mut store,
                        &identifier,
                        &agent_id,
                        branch.as_deref(),
                        cli.json,
                    )?,
                    Commands::Heartbeat {
                        identifier,
                        agent_id,
                    } => handle_heartbeat(&mut store, &identifier, &agent_id, cli.json)?,
                    Commands::Reap { dry_run } => handle_reap(&mut store, dry_run, cli.json)?,
                    Commands::Handoff(args) => handle_handoff(&mut store, args, cli.json)?,
                    Commands::Defer(args) => handle_defer(&mut store, args, cli.json)?,
                    Commands::Link {
                        source,
                        target,
                        kind,
                    } => handle_link(&mut store, &source, &target, &kind, cli.json)?,
                    Commands::Split(args) => handle_split(&mut store, args, cli.json)?,
                    Commands::AskDecision(args) => handle_ask_decision(&mut store, args, cli.json)?,
                    Commands::Decide(args) => handle_decide(&mut store, args, cli.json)?,
                    Commands::SupersedeDecision {
                        identifier,
                        decision_id,
                        reason,
                    } => handle_supersede_decision(
                        &mut store,
                        &identifier,
                        &decision_id,
                        reason,
                        cli.json,
                    )?,
                    Commands::Verify(args) => handle_verify(&mut store, args, cli.json)?,
                    Commands::Update(args) => handle_update(&mut store, args, cli.json)?,
                    Commands::Plan(args) => handle_plan(&mut store, args, cli.json)?,
                    Commands::Graph { format } => handle_graph(&store, &format, cli.json)?,
                    Commands::Migrate(args) => crate::handlers::migrate_cmds::handle_migrate(
                        &root_dir,
                        args.dry_run,
                        args.scope_only,
                        cli.json,
                    )?,
                    Commands::Spool(args) => match args.command {
                        SpoolCommands::Migrate { path, dry_run } => {
                            handle_spool_migrate(&store, path.as_deref(), dry_run, cli.json)?;
                        }
                    },
                    Commands::Archive { identifier } => {
                        handle_archive(&mut store, &identifier, cli.json)?
                    }
                    Commands::Unarchive { identifier } => {
                        handle_unarchive(&mut store, &identifier, cli.json)?
                    }
                    Commands::Park { identifier, reason } => {
                        handle_park(&mut store, &identifier, reason.as_deref(), cli.json)?
                    }
                    Commands::Unpark { identifier } => {
                        handle_unpark(&mut store, &identifier, cli.json)?
                    }
                    Commands::Reopen { identifier, reason } => {
                        handle_reopen(&mut store, &identifier, &reason, cli.json)?
                    }
                }
            }
        }

        Ok(())
    })();

    invocation.finish(result.is_ok());
    result
}
