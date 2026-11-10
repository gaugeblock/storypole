//! Recording what a remint moved, so a later pass can resolve a reference to it (`GB-347`).
//!
//! A remint that does not record its mapping makes every cross-store question archaeology: the harness store cites a
//! member key, the member key moved, and the only remaining evidence of what it meant is a rename inside a commit. The
//! map is small, mechanical, and written where the tickets are, so the record travels with the store it describes.
//!
//! Identity is the task uuid. A key is a name and names change; the uuid is what survives, and it is what a link
//! written afterwards must point at.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use storypole_core::store::StorypoleStore;
use uuid::Uuid;

/// One key that moved, with the identity that survived it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedRemint {
    /// The task the key belonged to.
    pub id: Uuid,
    /// The key as it was before the remint.
    pub from: String,
    /// The key it carries now.
    pub to: String,
}

/// The on-disk shape, wrapped so the file can grow a field without breaking a reader.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Record {
    remints: Vec<AppliedRemint>,
}

/// Where the record lives: beside the tickets, in the store's own dot-directory.
fn path(store: &Path) -> PathBuf {
    StorypoleStore::resolve_dot_dir(store).join("remints.json")
}

/// Every remint this store has recorded, oldest first.
///
/// # Errors
///
/// Returns an error when the file exists but cannot be read or parsed - a record that cannot be understood is a
/// failure to report, because silently treating it as empty would let a later pass resolve references against nothing.
pub fn load(store: &Path) -> Result<Vec<AppliedRemint>> {
    let path = path(store);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("could not read {}", path.display()))?;
    let record: Record = serde_json::from_str(&text)
        .with_context(|| format!("{} is not a readable remint record", path.display()))?;
    Ok(record.remints)
}

/// Merges `fresh` into the store's record, returning how many entries were new.
///
/// Idempotent by `(uuid, to)`: re-running a remint cannot duplicate what it already recorded.
///
/// # Errors
///
/// Returns an error when the existing record cannot be read or the merged one cannot be written.
pub fn append(store: &Path, fresh: &[AppliedRemint]) -> Result<usize> {
    if fresh.is_empty() {
        return Ok(0);
    }
    let mut existing = load(store)?;
    let before = existing.len();
    for entry in fresh {
        if !existing
            .iter()
            .any(|known| known.id == entry.id && known.to == entry.to)
        {
            existing.push(entry.clone());
        }
    }
    let added = existing.len() - before;
    if added == 0 {
        return Ok(0);
    }
    existing.sort_by(|left, right| left.to.cmp(&right.to));
    let path = path(store);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&Record { remints: existing })?,
    )
    .with_context(|| format!("could not write {}", path.display()))?;
    Ok(added)
}
