//! The `GB-347` key migration: moving keys, and moving what points at them.
//!
//! Split by question rather than by step. [`rewrite`] answers "how does one string change", [`events`] answers
//! "which fields does replay actually read", and [`verify`] answers "did it work" - against replay, not against the
//! file the pass just wrote. The third question is the one the first implementation of this verb did not ask, and
//! it is why nine stores were still broken while `remint --apply` reported success (`GB-347`).

pub mod events;
pub mod record;
pub mod rewrite;
pub mod verify;

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod verify_tests;
