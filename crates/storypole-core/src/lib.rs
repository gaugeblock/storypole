//! Core library for `storypole`: Git-native task DAG, backlog, decision store, and ledger.

#![deny(unsafe_code)]

pub mod config;
pub mod config_audit;
pub mod dag;
pub mod error;
pub mod git;
pub mod index;
pub mod leases;
pub mod ledger;
pub mod migrate;
pub mod model;
pub mod ops;
pub mod state;
pub mod storage_mode;
pub mod store;
mod store_locate;
pub mod store_records;
pub mod telemetry;
pub mod ticket_keys;
pub mod tickets;
pub mod workspace;

pub use config::StorypoleConfig;
pub use error::StorypoleError;
pub use index::query::{TaskFilter, resolve_task_id};
pub use model::*;
pub use store::StorypoleStore;
pub use workspace::{DiscoveredMember, StorypoleWorkspaceStore, WorkspaceConfig, discover_members};

/// Crate version constant.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
