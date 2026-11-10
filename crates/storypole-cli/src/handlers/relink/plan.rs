//! What a cross-store repair would change, computed without writing (`GB-347`).
//!
//! "Dead" is precise here, and the precision is the whole safety argument: a key is dead when some store recorded it as
//! having moved **and** no store holds it today. A key the harness still uses is not dead, so the six keys two stores
//! once shared resolve to the harness and are left alone - the ambiguity stays unresolved rather than guessed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::Value;
use storypole_core::store::StorypoleStore;
use storypole_core::ticket_keys::Remint;
use storypole_core::workspace::discovery::discover_members;

use crate::handlers::remint::events::rewrite_event;
use crate::handlers::remint::rewrite::{Rewrite, rewrite_task};

/// A ticket that a dead key used to name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Repository that owns the ticket now.
    pub store: String,
    /// Key the ticket carries now.
    pub key: String,
    /// The task uuid, which no rename touches.
    pub id: String,
}

/// A ticket whose served text still cites a key that moved.
#[derive(Debug, Clone)]
pub struct Repair {
    /// Repository holding the ticket.
    pub store: String,
    /// The ticket's current key.
    pub key: String,
}

/// A harness ticket and the member ticket carrying the work it describes.
#[derive(Debug, Clone)]
pub struct Link {
    /// The harness ticket, by current key.
    pub harness: String,
    /// The member ticket doing the work.
    pub target: Target,
}

/// Everything the pass would do, and what it refuses to guess.
#[derive(Debug, Default)]
pub struct Plan {
    /// Dead key to the ticket it now names.
    pub moved: BTreeMap<String, Target>,
    /// Dead keys two stores both claim, left alone rather than resolved by coin toss.
    pub conflicts: Vec<String>,
    /// Tickets whose text changes.
    pub repairs: Vec<Repair>,
    /// Links to write.
    pub links: Vec<Link>,
}

/// The stores this suite answers for: its members, then the harness itself.
pub fn stores(root: &Path) -> Vec<(String, PathBuf)> {
    let mut stores: Vec<(String, PathBuf)> = discover_members(root, None)
        .into_iter()
        .map(|member| (member.name, member.repo_path))
        .collect();
    stores.push((".".to_string(), root.to_path_buf()));
    stores
}

/// Every ticket file in one store.
pub fn ticket_files(store: &Path) -> Vec<PathBuf> {
    let dir = StorypoleStore::resolve_dot_dir(store).join("tickets");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    paths
}

/// The keys a store holds today.
pub fn ticket_keys(store: &Path) -> BTreeSet<String> {
    ticket_files(store)
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .filter_map(|text| serde_json::from_str::<Value>(&text).ok())
        .filter_map(|document| {
            document
                .get("ticket")?
                .get("ticket_key")?
                .as_str()
                .map(str::to_string)
        })
        .collect()
}

/// Rewrites one ticket document, returning `(edges, citations)` it would change.
///
/// Shared with the apply pass so the plan cannot promise something the apply does not do - the failure mode the
/// remint's first version shipped with.
///
/// Two things differ from a remint's rewrite, both from `GB-351`. **Nothing moves on disk here**, so a plan's stored path
/// is left as written; a remint renames the blueprint beside the ticket and would otherwise make this pass point at a
/// file that never moved. And **only the payload that wins on replay is rewritten**: replay keeps the last
/// `task_updated` description, so rewriting a superseded one edits history for no effect on state - which kept a
/// corrected ticket reporting work to do.
pub fn rewrite_document(document: &mut Value, mapping: &[Remint]) -> (usize, usize) {
    let lookup: BTreeMap<String, String> = mapping
        .iter()
        .map(|remint| (remint.from.clone(), remint.to.clone()))
        .collect();
    let (mut edges, mut citations) = (0, 0);
    if let Some(ticket) = document.get_mut("ticket") {
        let (e, c) = rewrite_task(ticket, &lookup, mapping, Rewrite::RELINK);
        edges += e;
        citations += c;
    }
    if let Some(events) = document.get_mut("events").and_then(Value::as_array_mut) {
        let winning_update = events
            .iter()
            .rposition(|event| event.get("event").and_then(Value::as_str) == Some("task_updated"));
        for (index, event) in events.iter_mut().enumerate() {
            let is_update = event.get("event").and_then(Value::as_str) == Some("task_updated");
            if is_update && Some(index) != winning_update {
                continue;
            }
            let (e, c) = rewrite_event(event, &lookup, mapping, Rewrite::RELINK);
            edges += e;
            citations += c;
        }
    }
    (edges, citations)
}
