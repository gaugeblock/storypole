//! `storypole refresh-projections` - bring ticket blocks back in line with their events (`GB-349`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use storypole_core::store::StorypoleStore;
use storypole_core::tickets::{RefreshReport, refresh_store};

use crate::json_out::print_json;

/// Repairs every store under `root_dir`, or reports what a repair would touch.
///
/// A suite root and a single repository both work: the root store and each discovered member store are
/// walked, because the divergence this repairs was measured across both (71 stale blocks, spread over
/// eleven repositories).
pub fn handle_refresh_projections(root_dir: &Path, apply: bool, json: bool) -> Result<()> {
    let mut stores: Vec<(String, PathBuf)> = Vec::new();
    stores.push((
        "root".to_string(),
        StorypoleStore::resolve_dot_dir(root_dir),
    ));
    for member in storypole_core::workspace::discovery::discover_members(root_dir, None) {
        if let Some(dot_dir) = member.dot_dir {
            stores.push((member.name, dot_dir));
        }
    }

    let mut total = RefreshReport::default();
    let mut per_store: Vec<(String, RefreshReport)> = Vec::new();
    for (name, dot_dir) in stores {
        let report = refresh_store(&dot_dir, apply).with_context(|| {
            format!("refresh failed for store '{name}' at {}", dot_dir.display())
        })?;
        total.scanned += report.scanned;
        total.stale.extend(report.stale.iter().cloned());
        total.written.extend(report.written.iter().cloned());
        total.unreadable.extend(report.unreadable.iter().cloned());
        if !report.is_clean() {
            per_store.push((name, report));
        }
    }

    if json {
        let stores: Vec<serde_json::Value> = per_store
            .iter()
            .map(|(name, report)| {
                serde_json::json!({
                    "store": name,
                    "stale": report.stale,
                    "written": report.written,
                    "unreadable": report.unreadable.iter()
                        .map(|(key, reason)| serde_json::json!({"key": key, "reason": reason}))
                        .collect::<Vec<_>>(),
                })
            })
            .collect();
        print_json(
            "refresh-projections",
            &serde_json::json!({
                "refresh": {
                    "applied": apply,
                    "scanned": total.scanned,
                    "stale": total.stale.len(),
                    "written": total.written.len(),
                    "unreadable": total.unreadable.len(),
                    "stores": stores,
                }
            }),
        )?;
    } else if total.is_clean() {
        println!(
            "PROJECTIONS AGREE: {} record(s) scanned, none disagree with their events",
            total.scanned
        );
    } else {
        for (name, report) in &per_store {
            println!(
                "{name}: {} stale, {} written, {} unreadable",
                report.stale.len(),
                report.written.len(),
                report.unreadable.len()
            );
            for key in &report.stale {
                println!("  {key}");
            }
            for (key, reason) in &report.unreadable {
                println!("  {key}: unreadable - {reason}");
            }
        }
        if !apply {
            println!(
                "{} record(s) disagree with their events; re-run with --apply to rewrite them",
                total.stale.len()
            );
        }
    }

    if total.unreadable.is_empty() {
        return Ok(());
    }
    anyhow::bail!(
        "{} record(s) could not be read; they were not repaired and need a look",
        total.unreadable.len()
    )
}
