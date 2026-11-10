//! The chain inheritance rule (`SP-1`).
//!
//! A member repository declares no verification chains of its own - they live once at the suite root, keyed by the
//! directory names the `[suite] members` roster uses - so opening a member's store inherits them. What these tests
//! pin is the *direction* of that inheritance: it fills a gap and never overrides a decision made closer to the code.

use storypole_core::config::StorypoleConfig;

fn chain(key: &str) -> (String, String) {
    (key.to_string(), format!("echo {key}"))
}

#[test]
fn a_repository_with_no_chains_inherits_the_suites() {
    let local = StorypoleConfig::default();
    assert!(local.verification_commands.is_empty());

    let inherited = local.inheriting(vec![chain("common"), chain("default")]);

    assert_eq!(
        inherited.verification_chain_exact("common").map(|(k, _)| k),
        Some("common"),
        "a member store with no chains of its own must see the suite's, or its tasks fall back to cargo test"
    );
    assert!(inherited.verification_chain_default().is_some());
}

#[test]
fn a_repository_that_declares_chains_keeps_all_of_them() {
    // All or nothing on purpose. Key-by-key merging would let a member accumulate half a chain from its own file
    // and half from the suite - a configuration nobody wrote and nobody could predict.
    let local = StorypoleConfig {
        verification_commands: vec![chain("member-local")],
        ..StorypoleConfig::default()
    };

    let after = local.inheriting(vec![chain("common"), chain("default")]);

    assert!(
        after.verification_chain_exact("member-local").is_some(),
        "a local declaration must survive"
    );
    assert!(
        after.verification_chain_exact("common").is_none(),
        "and it must not be joined by the suite's chains, which would make the effective config unreadable"
    );
    assert_eq!(after.verification_commands.len(), 1);
}

#[test]
fn a_lone_checkout_is_unchanged_by_an_empty_suite() {
    // `chains_declared_by_suite` returns nothing outside a suite, and this is what the store then does with it.
    let local = StorypoleConfig::default();
    let after = local.inheriting(Vec::new());

    assert!(after.verification_commands.is_empty());
    assert_eq!(after.verification_chain_default(), None);
}
