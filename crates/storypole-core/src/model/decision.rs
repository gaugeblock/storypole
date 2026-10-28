//! Decision, lease and verification wire types, re-exported from the shared contracts crate.
//!
//! The definitions moved to `gaugeblock-contracts` (GB-143), including `AgentClaim::is_expired`,
//! which is pure lease arithmetic and therefore travels with the type.

pub use gaugeblock_contracts::{
    AgentClaim, DecisionStatus, HandoffPacket, TaskDecision, VerificationReceipt,
};
