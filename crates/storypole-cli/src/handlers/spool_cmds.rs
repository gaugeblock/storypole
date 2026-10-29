//! Telemetry spool maintenance command handlers.

use std::path::Path;

use anyhow::{Context, Result};
use storypole_core::store::StorypoleStore;
use storypole_core::telemetry::{migrate_spool, normalize_spool_text, spool_path};

/// Rewrites legacy bare-trace outbox lines into canonical `SpoolEntry` envelopes.
///
/// Resolves `<canonical .gaugeblock>/spool/outbox.jsonl` unless `path` overrides it. With `dry_run`
/// the rewrite is computed and counted but the file is left untouched.
pub fn handle_spool_migrate(
    store: &StorypoleStore,
    path: Option<&Path>,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    let outbox = path.map_or_else(|| spool_path(store.dot_dir()), Path::to_path_buf);

    let migrated = if dry_run {
        let text = if outbox.exists() {
            std::fs::read_to_string(&outbox)
                .with_context(|| format!("spool outbox cannot be read: {}", outbox.display()))?
        } else {
            String::new()
        };
        normalize_spool_text(&text)
            .with_context(|| format!("spool outbox cannot be rebuilt: {}", outbox.display()))?
            .1
    } else {
        migrate_spool(&outbox)
            .with_context(|| format!("spool outbox cannot be migrated: {}", outbox.display()))?
    };

    if json {
        let payload = serde_json::json!({
            "outbox": outbox.display().to_string(),
            "migrated": migrated,
            "dry_run": dry_run,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if migrated == 0 {
        println!(
            "{}: no legacy records to migrate (already canonical)",
            outbox.display()
        );
    } else if dry_run {
        println!(
            "{}: {migrated} legacy record(s) would be rewritten (dry run, file untouched)",
            outbox.display()
        );
    } else {
        println!(
            "{}: {migrated} legacy record(s) rewritten as SpoolEntry envelopes",
            outbox.display()
        );
    }
    Ok(())
}
