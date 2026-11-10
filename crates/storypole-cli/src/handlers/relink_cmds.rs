//! `storypole relink` - repair what a per-store remint could not see, and link the work (`GB-347`).
//!
//! Dry run by default: the verb's job is to make a cross-store repair reviewable before it happens, because its evidence
//! is prose and prose is where a migration is most likely to be wrong.

use std::path::Path;

use anyhow::Result;

use super::relink::{apply as apply_repair, build};
use crate::json_out::print_json;

/// Reports the cross-store plan, or applies it with `write`.
///
/// # Errors
///
/// Returns an error when the plan cannot be built, or when applying it fails or cannot prove itself.
pub fn handle_relink(root_dir: &Path, write: bool, json: bool) -> Result<()> {
    let planned = build::build(root_dir)?;

    if json {
        let payload = serde_json::json!({
            "moved": planned.moved.len(),
            "conflicts": planned.conflicts,
            "repairs": planned.repairs.iter().map(|repair| serde_json::json!({
                "store": repair.store, "ticket": repair.key
            })).collect::<Vec<_>>(),
            "links": planned.links.iter().map(|link| serde_json::json!({
                "harness": link.harness, "store": link.target.store, "ticket": link.target.key
            })).collect::<Vec<_>>(),
        });
        print_json("relink", &payload)?;
        return Ok(());
    }

    for conflict in &planned.conflicts {
        println!("left alone, two stores claim it: {conflict}");
    }
    // `moved` stays populated forever - the dead keys never come back - so what makes the pass finished is that nothing
    // cites one any more. Saying "re-run with --apply" here would invite a second run of a job with no work in it.
    if planned.repairs.is_empty() && planned.links.is_empty() {
        println!(
            "nothing to relink: {} key(s) moved, and no ticket cites one",
            planned.moved.len()
        );
        return Ok(());
    }

    println!(
        "{} key(s) moved and no longer exist; {} ticket(s) cite one; {} link(s) to write",
        planned.moved.len(),
        planned.repairs.len(),
        planned.links.len()
    );
    for repair in &planned.repairs {
        println!("  repairs {} {}", repair.store, repair.key);
    }
    for link in &planned.links {
        println!(
            "  links   {} -> {} {}",
            link.harness, link.target.store, link.target.key
        );
    }

    if !write {
        println!("\nNothing has been changed. Re-run with --apply to repair and link.");
        return Ok(());
    }

    let applied = apply_repair::apply(root_dir, &planned)?;
    println!(
        "\n{} ticket(s) repaired: {} citation(s), {} edge target(s) rewritten, {} link(s) written",
        applied.tickets, applied.citations, applied.edges, applied.links
    );
    Ok(())
}
