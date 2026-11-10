//! Planning a key remint (`GB-347`).
//!
//! The mapping decides what 195 tickets across eleven stores will be called. These tests pin the two properties that
//! make it safe: never collide with a key that is already correct, and be ordered the way a person reads numbers.

use storypole_core::ticket_keys::plan_remint;

fn keys(items: &[&str]) -> Vec<String> {
    items.iter().map(ToString::to_string).collect()
}

#[test]
fn numbering_continues_from_the_highest_key_already_using_the_prefix() {
    // `SP-2` and `SP-7` are correct already. A remint must not hand out `SP-1`.
    let mapping = plan_remint("SP", &keys(&["GB-100", "SP-2", "GB-9", "SP-7"]));

    assert_eq!(mapping.len(), 2, "only the off-prefix keys move");
    let assigned: Vec<&str> = mapping.iter().map(|r| r.to.as_str()).collect();
    assert_eq!(assigned, vec!["SP-8", "SP-9"]);
}

#[test]
fn keys_are_reminted_in_numeric_order_not_string_order() {
    // The bug this prevents: `GB-10` before `GB-9` because '1' < '9', which makes the mapping look arbitrary on review.
    let mapping = plan_remint("CO", &keys(&["GB-10", "GB-9", "GB-100"]));

    let order: Vec<&str> = mapping.iter().map(|r| r.from.as_str()).collect();
    assert_eq!(order, vec!["GB-9", "GB-10", "GB-100"]);
}

#[test]
fn a_store_that_is_already_correct_plans_nothing() {
    assert!(plan_remint("BA", &keys(&["BA-1", "BA-2"])).is_empty());
}

#[test]
fn an_empty_store_plans_nothing_rather_than_starting_at_one() {
    assert!(plan_remint("RB", &[]).is_empty());
}
