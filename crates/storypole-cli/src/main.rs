//! Command-line interface and entrypoint for `storypole`.

#![deny(unsafe_code)]

mod args;
mod format;
mod handlers;
mod plan_args;
mod update_args;

use anyhow::Result;
use args::{Cli, Commands};
use clap::Parser;
use handlers::*;
use std::env;
use storypole_core::store::StorypoleStore;

fn main() -> Result<()> {
    let cli = Cli::parse();

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

    match cli.command {
        Commands::Init(args) => handle_init(&root_dir, args, cli.json)?,
        Commands::Mcp => {
            storypole_mcp::run_stdio_server(&root_dir)
                .map_err(|e| anyhow::anyhow!("MCP server error: {e}"))?;
        }
        cmd => {
            let mut store = StorypoleStore::open(&root_dir)?;
            match cmd {
                Commands::Init(_) | Commands::Mcp => unreachable!(),
                Commands::Add(args) => handle_add(&mut store, args, cli.json)?,
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
            }
        }
    }

    Ok(())
}
