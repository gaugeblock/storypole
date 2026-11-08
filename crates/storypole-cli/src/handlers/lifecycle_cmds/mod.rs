//! Lifecycle and decision command handlers: claim, heartbeat, handoff, decisions, verify.

use crate::args::*;
use crate::format::format_task_briefing;
use crate::json_out::print_json;
use anyhow::{Result, anyhow};
use storypole_core::index::query::resolve_task_id;
use storypole_core::model::decision::HandoffPacket;
use storypole_core::model::task::TaskStatus;
use storypole_core::ops::decisions::{ask_decision, decide, supersede_decision};
use storypole_core::ops::workflow::{claim_task, handoff_task, heartbeat, verify_task};
use storypole_core::store::StorypoleStore;
use uuid::Uuid;

pub fn handle_claim(
    store: &mut StorypoleStore,
    ident: &str,
    agent_id: &str,
    branch: Option<&str>,
    json: bool,
) -> Result<()> {
    let id = resolve_task_id(store.conn(), ident)?;
    let briefing = claim_task(store, id, agent_id, branch)?;
    if json {
        print_json("claim", &serde_json::json!({"briefing": briefing}))?;
    } else {
        println!("Task {ident} claimed by {agent_id}. Briefing:\n");
        print!("{}", format_task_briefing(&briefing));
    }
    Ok(())
}

pub fn handle_heartbeat(
    store: &mut StorypoleStore,
    ident: &str,
    agent_id: &str,
    json: bool,
) -> Result<()> {
    let id = resolve_task_id(store.conn(), ident)?;
    let expires_at = heartbeat(store, id, agent_id)?;
    if json {
        println!("{{\"lease_expires_at_epoch_ms\":{expires_at}}}");
    } else {
        println!("Heartbeat acknowledged for {ident}. Lease expires at epoch ms: {expires_at}");
    }
    Ok(())
}

pub fn handle_reap(store: &mut StorypoleStore, dry_run: bool, json: bool) -> Result<()> {
    // Dry run reports the same set without recording anything: a read that writes would make `list`
    // mutate the ledger and would fail on a read-only checkout.
    let reaped = if dry_run {
        storypole_core::ops::lease::expired_claims(store)?
    } else {
        storypole_core::ops::lease::reap_expired_claims(store)?
    };

    if json {
        let rows: Vec<serde_json::Value> = reaped
            .iter()
            .map(|item| {
                serde_json::json!({
                    "ticket_key": item.ticket_key,
                    "agent_id": item.agent_id,
                    "expired_at_epoch_ms": item.expired_at_epoch_ms,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({ "dry_run": dry_run, "count": reaped.len(), "reaped": rows })
        );
        return Ok(());
    }

    if reaped.is_empty() {
        println!("No expired claim leases; every InProgress task has a live holder.");
        return Ok(());
    }

    if dry_run {
        println!(
            "[dry-run] {} task(s) would be returned to the backlog:",
            reaped.len()
        );
    } else {
        println!("Returned {} task(s) to the backlog:", reaped.len());
    }
    for item in &reaped {
        println!(
            "  {} (agent {}, lease expired at epoch ms {})",
            item.ticket_key, item.agent_id, item.expired_at_epoch_ms
        );
    }
    Ok(())
}

pub fn handle_handoff(store: &mut StorypoleStore, args: HandoffArgs, json: bool) -> Result<()> {
    let id = resolve_task_id(store.conn(), &args.identifier)?;
    let packet = HandoffPacket {
        recorded_by_agent: args.agent_id,
        timestamp_ms: StorypoleStore::now_ms(),
        completed_items: args.completed,
        remaining_items: args.remaining,
        last_error_or_blocker: args.blocker,
        git_ref: args.git_ref,
        dirty_files: args.dirty_files,
    };
    let new_status = match args.status.to_lowercase().as_str() {
        "blocked" => TaskStatus::Blocked,
        "failed" => TaskStatus::Failed,
        _ => TaskStatus::Ready,
    };
    let effective_status = handoff_task(store, id, packet, new_status)?;
    if effective_status != new_status {
        eprintln!(
            "note: {} still waits on a pending decision; handoff requested {:?} but the task remains {:?}",
            args.identifier, new_status, effective_status
        );
    }
    if json {
        println!("{{\"success\":true,\"status\":\"{:?}\"}}", effective_status);
    } else {
        println!(
            "Recorded handoff packet for {}. Task moved to {:?}",
            args.identifier, effective_status
        );
    }
    Ok(())
}

pub fn handle_verify(store: &mut StorypoleStore, args: VerifyArgs, json: bool) -> Result<()> {
    let id = resolve_task_id(store.conn(), &args.identifier)?;
    let override_receipt = if args.verifier.is_some() || args.summary.is_some() {
        Some(storypole_core::model::decision::VerificationReceipt {
            verifier: args.verifier.unwrap_or_else(|| "manual".to_string()),
            command: "n/a".to_string(),
            exit_code: Some(0),
            diagnostic_count: None,
            summary: args
                .summary
                .unwrap_or_else(|| "Manual verification confirmed".to_string()),
            verified_at_epoch_ms: StorypoleStore::now_ms(),
            git_branch: None,
            base_commit_sha: None,
            head_commit_sha: None,
            commit_shas: Vec::new(),
            repo: None,
            chain_key: None,
        })
    } else {
        None
    };
    // Executing is the default (`GB-272`): a certification command that can return exit_code 0 without
    // running anything produces receipts that look like proof. `--manual` is the explicit opt-out.
    let run_command = !args.manual;
    let receipt = verify_task(
        store,
        id,
        override_receipt,
        run_command || args.run,
        args.compiler,
    )?;
    if json {
        print_json("verify", &serde_json::json!({"receipt": receipt}))?;
    } else {
        let branch_info = receipt
            .git_branch
            .as_deref()
            .map(|b| format!(" on branch '{b}'"))
            .unwrap_or_default();
        let commits_info = if !receipt.commit_shas.is_empty() {
            format!(" [{} commit(s)]", receipt.commit_shas.len())
        } else {
            String::new()
        };
        let diag_info = receipt
            .diagnostic_count
            .map(|d| format!(", diagnostics: {d}"))
            .unwrap_or_default();
        // The repository and chain are reported on the same line as the branch: a reader who asked for
        // `miter` verification needs to see whether the member chain or the harness chain ran (`GB-316`).
        let repo_info = receipt
            .repo
            .as_deref()
            .map(|r| format!(" on repo '{r}'"))
            .unwrap_or_else(|| " on no repository".to_string());
        let chain_info = receipt
            .chain_key
            .as_deref()
            .map(|c| format!(", chain: {c}"))
            .unwrap_or_default();
        println!(
            "Verified task {} successfully (verifier: {}, exit_code: {:?}{diag_info}{repo_info}{branch_info}{commits_info}{chain_info})",
            args.identifier, receipt.verifier, receipt.exit_code
        );
    }
    Ok(())
}

mod decisions;
pub use decisions::{handle_ask_decision, handle_decide, handle_supersede_decision};
