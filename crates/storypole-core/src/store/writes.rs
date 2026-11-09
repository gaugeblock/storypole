//! Writing to a store, whichever layout it uses (`GB-329`, `GB-345`).
//!
//! Split from `store::mod` for the line limit. A child module can see the parent's private fields, so the write
//! path stays an inherent method on the store rather than a free function that has to be handed everything.

use crate::error::StorypoleError;
use crate::index::{open_and_sync_index, open_and_sync_records};
use crate::ledger::append_event;
use crate::model::LedgerEvent;
use crate::storage_mode::StorageMode;

use super::StorypoleStore;

impl StorypoleStore {
    /// Record a ledger event, sync SQLite index, and persist to disk.
    pub fn record_event(&mut self, event: &LedgerEvent) -> Result<(), StorypoleError> {
        match self.mode {
            StorageMode::Ledger => {
                let ledger = self.ledger_path();
                append_event(&ledger, event)?;
                self.conn = open_and_sync_index(&self.sqlite_path(), &ledger)?;
            }
            StorageMode::Records => {
                // One target only. Writing both layouts would be the divergence GB-79 documents.
                crate::store_records::record_event(&self.conn, &self.dot_dir, event)?;
                self.conn = open_and_sync_records(&self.sqlite_path(), &self.dot_dir)?;
            }
        }
        Ok(())
    }

    /// Records the creation of a task, reporting whether the ticket key was free (`GB-345`).
    ///
    /// Records mode publishes exclusively, so a `false` means retry with the next key rather than overwrite a
    /// ticket; the ledger has no exclusive append and answers `true`, a known limitation of that layout.
    pub fn record_creation(&mut self, event: &LedgerEvent) -> Result<bool, StorypoleError> {
        match self.mode {
            StorageMode::Ledger => {
                self.record_event(event)?;
                Ok(true)
            }
            StorageMode::Records => {
                let created = crate::store_records::create_task(&self.dot_dir, event)?;
                if created {
                    self.conn = open_and_sync_records(&self.sqlite_path(), &self.dot_dir)?;
                }
                Ok(created)
            }
        }
    }
}
