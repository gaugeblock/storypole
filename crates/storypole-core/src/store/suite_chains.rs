//! The verification chains a suite declares, for the members that inherit them (`SP-1`).
//!
//! Separate from `store` because the store began to cross the 250-line limit when this arrived, and because the
//! question it answers - "what did the suite declare?" - is not about a store at all. Two callers use it: the store
//! construction path, which inherits, and the workspace audit, which must inherit without opening a store.

use std::path::Path;

/// The verification chains declared by the suite that contains `root_dir` (`SP-1`).
///
/// Returns nothing for a lone checkout, which is why the caller can inherit unconditionally: a repository that is
/// not in a suite keeps exactly the configuration it had before. A malformed suite config also yields nothing -
/// the suite's own tooling is the right place to complain about it, and a member should not refuse to open because
/// a file it does not own is broken.
pub fn chains_declared_by_suite(root_dir: &Path) -> Vec<(String, String)> {
    let Some(membership) = gaugeblock_workspace::Membership::containing(root_dir) else {
        return Vec::new();
    };
    let path = gaugeblock_workspace::GaugeblockPaths::at(&membership.suite_root).config();
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(table) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let Some(entries) = table
        // The chains live under `[storypole]`, not at the top level: the suite config opens that section a few
        // lines above `verification_commands`. Reading the root table - which the first version of this function
        // did - finds nothing and inherits nothing, silently, which is the same wrong-place lookup this ticket was
        // filed about (`SP-1`).
        .get("storypole")
        .and_then(|value| value.as_table())
        .and_then(|section| section.get("verification_commands"))
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };
    // Each entry is `[key, command]`. Anything else is skipped rather than fatal: the caller falls back to the
    // previous behaviour, which is a chain that does not run - the status quo, not a new failure.
    entries
        .iter()
        .filter_map(|entry| {
            let pair = entry.as_array()?;
            let key = pair.first()?.as_str()?;
            let command = pair.get(1)?.as_str()?;
            Some((key.to_string(), command.to_string()))
        })
        .collect()
}
