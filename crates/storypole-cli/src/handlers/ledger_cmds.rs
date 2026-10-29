//! Ledger integrity audit command handler.

use std::path::Path;

use anyhow::{Context, Result};
use storypole_core::ledger::audit::{audit_ledgers, format_ledger_audit};

/// Audits every ledger under `root_dir` and reports structural health plus the replayed statuses.
pub fn handle_ledger_verify(root_dir: &Path, json: bool) -> Result<()> {
    let audit = audit_ledgers(root_dir)
        .with_context(|| format!("Ledger audit failed for {}", root_dir.display()))?;

    if json {
        println!("{}", serde_json::to_string_pretty(&audit)?);
    } else {
        print!("{}", format_ledger_audit(&audit));
    }

    if !audit.is_structurally_sound() {
        anyhow::bail!(
            "ledger set is not structurally sound: {} unparsable, {} duplicate",
            audit.unparsable,
            audit.duplicates
        );
    }
    Ok(())
}
