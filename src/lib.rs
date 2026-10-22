//! # storypole
//!
//! Git-native SQLite and JSONL backlog, task DAG, and MCP memory store for AI coding agents.

#![deny(unsafe_code)]

pub use storypole_core::*;

/// Storypole crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
