//! Which file layout a store reads and writes (`GB-329`).
//!
//! The suite stored every ticket as a line in one append-only ledger. A ticket is now a file, and the two
//! layouts have to coexist while the migration (`GB-331`) has not run: a repository with a ledger keeps
//! reading and writing the ledger, and a repository with a `tickets/` directory reads and writes records.
//!
//! Exactly one of them is authoritative at a time. Writing both would recreate the divergence `GB-79`
//! documents - two copies of one task, reconciled by nobody - so the mode is chosen, never blended.

use std::path::Path;

use rusqlite::Connection;

use crate::error::StorypoleError;
use crate::index::{open_and_sync_index, open_and_sync_records};

use crate::tickets;

/// The authoritative ticket layout for one store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageMode {
    /// One append-only `ledger.jsonl`, which is what every repository has today.
    Ledger,
    /// One file per ticket under `tickets/`, which is what the migration produces.
    Records,
}

impl StorageMode {
    /// Records win when the directory exists, because only the migration creates it.
    ///
    /// Detecting by directory rather than by configuration keeps the state inspectable: `ls` explains the
    /// behaviour, and no flag can disagree with what is on disk.
    #[must_use]
    pub fn detect(dot_dir: &Path) -> Self {
        if tickets::dir(dot_dir).is_dir() {
            Self::Records
        } else {
            Self::Ledger
        }
    }
}

/// Opens the index for whichever layout `dot_dir` uses.
///
/// Lives here rather than in `store.rs` because the choice is the mode's, and because that file is at its
/// line limit.
pub(crate) fn sync_index(
    sqlite_path: &Path,
    dot_dir: &Path,
) -> Result<(Connection, StorageMode), StorypoleError> {
    let mode = StorageMode::detect(dot_dir);
    let conn = match mode {
        StorageMode::Records => open_and_sync_records(sqlite_path, dot_dir)?,
        StorageMode::Ledger => open_and_sync_index(sqlite_path, &dot_dir.join("ledger.jsonl"))?,
    };
    Ok((conn, mode))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn a_store_without_a_tickets_directory_reads_the_ledger() {
        let temp = tempfile::tempdir().expect("tempdir");
        assert_eq!(StorageMode::detect(temp.path()), StorageMode::Ledger);
    }

    #[test]
    fn a_store_with_a_tickets_directory_reads_records() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tickets::dir(temp.path())).expect("tickets dir");
        assert_eq!(StorageMode::detect(temp.path()), StorageMode::Records);
    }
}
