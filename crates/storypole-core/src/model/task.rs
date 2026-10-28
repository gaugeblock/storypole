//! Task-graph wire types, re-exported from the shared contracts crate.
//!
//! The definitions moved to `gaugeblock-contracts` (GB-143), and `TaskEdge`'s builders travelled
//! with the type. This module keeps the historical `storypole_core::model::task` path stable, so
//! the store, DAG and briefing code is untouched.

pub use gaugeblock_contracts::{EdgeKind, TaskEdge, TaskKind, TaskNode, TaskScope, TaskStatus};
