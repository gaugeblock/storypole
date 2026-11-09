//! Finding which store holds a ticket (`GB-330`).
//!
//! Since ADR 0009 a ticket lives in exactly one repository's store, so asking the suite root about a member's
//! ticket is a legitimate question with a real answer - and before this it answered `Task not found`, which is
//! indistinguishable from "no such ticket". The two are very different: one means you are in the wrong directory,
//! the other means the ticket does not exist.
//!
//! This is a *locator*, not a reader. It answers "where" so the caller can say something useful or open the right
//! store; reading across repositories is the workspace rollup's job, which needs the index rather than a path walk.

use std::path::{Path, PathBuf};

use crate::store::StorypoleStore;
use crate::tickets;
use crate::workspace::discovery::discover_members;

/// The store directory that holds `key`, searched across the workspace rooted at `workspace_root`.
///
/// The root store is checked first, because a suite-level ticket is the common case and a walk of eleven members
/// to answer it would be wasteful. Returns `None` when no store holds the key, which is what lets a caller keep
/// the original "not found" rather than inventing a location.
#[must_use]
pub fn locate_ticket(workspace_root: &Path, key: &str) -> Option<PathBuf> {
    let root_dot = StorypoleStore::resolve_dot_dir(workspace_root);
    if holds(&root_dot, key) {
        return Some(root_dot);
    }
    discover_members(workspace_root, None)
        .into_iter()
        .map(|member| StorypoleStore::resolve_dot_dir(&member.repo_path))
        .find(|dot| holds(dot, key))
}

/// Whether a store holds the ticket, in either layout.
fn holds(dot_dir: &Path, key: &str) -> bool {
    if tickets::path(dot_dir, key).is_ok_and(|path| path.is_file()) {
        return true;
    }
    // A repository still on the ledger needs the file searched rather than assumed absent.
    let ledger = dot_dir.join("ledger.jsonl");
    std::fs::read_to_string(ledger)
        .map(|text| text.contains(&format!("\"{key}\"")))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::fs;

    use super::*;

    fn member_with_ticket(root: &Path, name: &str, key: &str) {
        let dot = root.join(name).join(".gaugeblock").join("storypole");
        fs::create_dir_all(tickets::dir(&dot)).unwrap();
        fs::create_dir_all(dot.join("..").join("..")).unwrap();
        fs::write(tickets::dir(&dot).join(format!("{key}.json")), "{}").unwrap();
    }

    #[test]
    fn a_ticket_in_a_member_is_found_from_the_root() {
        // The exact case the migration created: GB-331 moved into storypole/, and the root must be able to say so.
        let temp = tempfile::tempdir().unwrap();
        member_with_ticket(temp.path(), "storypole", "GB-331");

        let found = locate_ticket(temp.path(), "GB-331").expect("located");

        assert_eq!(
            found,
            StorypoleStore::resolve_dot_dir(&temp.path().join("storypole"))
        );
    }

    #[test]
    fn an_unknown_key_has_no_location() {
        let temp = tempfile::tempdir().unwrap();
        member_with_ticket(temp.path(), "storypole", "GB-1");

        assert_eq!(locate_ticket(temp.path(), "GB-999"), None);
    }

    #[test]
    fn a_legacy_ledger_is_searched_too() {
        let temp = tempfile::tempdir().unwrap();
        let dot = temp
            .path()
            .join("benchwork")
            .join(".gaugeblock")
            .join("storypole");
        fs::create_dir_all(&dot).unwrap();
        fs::write(
            dot.join("ledger.jsonl"),
            "{\"event\":\"task_created\",\"task\":{\"ticket_key\":\"GB-42\"}}\n",
        )
        .unwrap();

        assert!(locate_ticket(temp.path(), "GB-42").is_some());
    }
}
