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

use crate::error::StorypoleError;
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
pub fn holds(dot_dir: &Path, key: &str) -> bool {
    if tickets::path(dot_dir, key).is_ok_and(|path| path.is_file()) {
        return true;
    }
    // A repository still on the ledger needs the file searched rather than assumed absent.
    let ledger = dot_dir.join("ledger.jsonl");
    std::fs::read_to_string(ledger)
        .map(|text| text.contains(&format!("\"{key}\"")))
        .unwrap_or(false)
}

/// The repository a store's dot-directory belongs to.
///
/// The canonical layout nests one level deeper than the legacy one, so a caller that needs to open the owning store from
/// a dot-directory - rather than only name it - has to undo that nesting (`GB-352`).
#[must_use]
pub fn repo_root_of(dot_dir: &Path) -> &Path {
    if dot_dir.ends_with(".gaugeblock/storypole") {
        dot_dir.parent().and_then(Path::parent).unwrap_or(dot_dir)
    } else {
        dot_dir.parent().unwrap_or(dot_dir)
    }
}

/// The suite root that `root` belongs to, or `root` itself when no suite claims it (`SP-48`).
///
/// Canonicalised before asking, for the same reason every other resolution does: `Membership::containing`
/// walks ancestors and a relative path stops at the current directory (`OF-42`).
#[must_use]
pub fn suite_root_of(root: &Path) -> PathBuf {
    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    gaugeblock_workspace::Membership::containing(&canonical)
        .map_or_else(|| canonical.clone(), |membership| membership.suite_root)
}

/// A task read from whichever store holds it, with the repository that store belongs to (`SP-48`).
#[derive(Debug)]
pub struct LocatedTask {
    /// The task itself, read from its owning store.
    pub task: crate::model::TaskNode,
    /// Repository name, from the task's own scope or from the directory that holds it.
    pub repo: Option<String>,
}

/// Read a task from whichever store in the suite holds it.
///
/// [`locate_ticket`] says *where*; this opens that store and answers *what*, which is what a cross-repository
/// edge needs in order to write a locator and to know whether the target still blocks. Returns `None` when no
/// store holds the key, so the caller keeps its own not-found wording.
///
/// # Errors
///
/// [`StorypoleError`] when the owning store cannot be opened or the record cannot be read. A key that is
/// located but not resolvable is `None`, not an error: the locator is a path walk and the index is the
/// authority, so a disagreement between them is answered by the authority.
pub fn find_task(root: &Path, key: &str) -> Result<Option<LocatedTask>, StorypoleError> {
    let Some(dot_dir) = locate_ticket(root, key) else {
        return Ok(None);
    };
    let repo_root = repo_root_of(&dot_dir);
    let store = StorypoleStore::open_exact(repo_root)?;
    let Ok(id) = crate::index::query::resolve_task_id(store.conn(), key) else {
        return Ok(None);
    };
    let task = crate::index::query::get_task_by_id(store.conn(), id)?;
    let repo = task.scope.repo.clone().or_else(|| {
        repo_root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    });
    Ok(Some(LocatedTask { task, repo }))
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
