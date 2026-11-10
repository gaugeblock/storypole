//! Cross-store repair and linking after a key migration (`GB-347`).
//!
//! A remint is deliberately one store at a time, which means it cannot see the references other stores hold. Those are
//! the leftovers: a harness ticket whose description says `GB-189` still says it after common's `GB-189` became `CO-7`,
//! and a member ticket may cite a sibling in another repository the same way.
//!
//! This pass is the other half, and it answers two questions with the same evidence. A citation of a key that moved is
//! a **broken pointer** and is repaired. When the citing ticket is a harness ticket, that citation is also the record
//! that a member ticket carries the work, and the model says such a ticket should be **linked**, not merely mentioned -
//! so the repair writes a `related_to` edge, by uuid, in the same pass.
//!
//! Repairing and linking together is not tidiness: a hand-written link would point at a key that a later remint renames,
//! while a link by uuid survives every rename.

pub mod apply;
pub mod build;
pub mod plan;

#[cfg(test)]
mod citation_tests;
#[cfg(test)]
mod tests;
