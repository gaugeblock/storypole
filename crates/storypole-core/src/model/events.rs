//! Ledger event and briefing wire types, re-exported from the shared contracts crate.
//!
//! `LedgerEvent` is the on-disk `ledger.jsonl` format. Its definition moved to
//! `gaugeblock-contracts` (GB-143) with the same 14 variants, the same `event` tag and the same
//! snake_case variant names, so every event already written still replays byte-identically.

pub use gaugeblock_contracts::{LedgerEvent, TaskContextBriefing};
