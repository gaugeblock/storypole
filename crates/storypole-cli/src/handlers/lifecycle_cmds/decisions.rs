//! Decision lifecycle commands: ask, decide, supersede.
//!
//! Split from the claim and handoff handlers when the file passed the 250-line limit (`GB-291`).

use super::*;
pub fn handle_ask_decision(
    store: &mut StorypoleStore,
    args: AskDecisionArgs,
    json: bool,
) -> Result<()> {
    let id = resolve_task_id(store.conn(), &args.identifier)?;
    let parent_id = match args.parent_id {
        Some(ref p) => Some(Uuid::parse_str(p)?),
        None => None,
    };
    let decision_id = ask_decision(
        store,
        id,
        args.category,
        args.question,
        args.options,
        parent_id,
    )?;
    if json {
        println!("{{\"decision_id\":\"{decision_id}\"}}");
    } else {
        println!(
            "Raised pending decision {decision_id} on {}. Task blocked.",
            args.identifier
        );
    }
    Ok(())
}

pub fn handle_decide(store: &mut StorypoleStore, args: DecideArgs, json: bool) -> Result<()> {
    let id = resolve_task_id(store.conn(), &args.identifier)?;
    let dec_id = Uuid::parse_str(&args.decision_id)
        .map_err(|e| anyhow!("Invalid decision UUID '{}': {e}", args.decision_id))?;
    decide(
        store,
        id,
        dec_id,
        args.choose,
        args.rationale,
        args.resolved_by,
        None,
        args.criterion,
    )?;
    if json {
        println!("{{\"success\":true}}");
    } else {
        println!(
            "Resolved decision {} on {}.",
            args.decision_id, args.identifier
        );
    }
    Ok(())
}

pub fn handle_supersede_decision(
    store: &mut StorypoleStore,
    ident: &str,
    dec_id_str: &str,
    reason: String,
    json: bool,
) -> Result<()> {
    let id = resolve_task_id(store.conn(), ident)?;
    let dec_id = Uuid::parse_str(dec_id_str)
        .map_err(|e| anyhow!("Invalid decision UUID '{dec_id_str}': {e}"))?;
    supersede_decision(store, id, dec_id, reason)?;
    if json {
        println!("{{\"success\":true,\"status\":\"superseded\"}}");
    } else {
        println!("Superseded decision {dec_id_str} on {ident}. Task moved back to Blocked.");
    }
    Ok(())
}
