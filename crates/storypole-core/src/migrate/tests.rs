//! Routing rules (`GB-331`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use gaugeblock_contracts::storypole::{TaskKind, TaskScope, TaskStatus};

use super::*;
use crate::ledger::replay::replay_events;
use crate::model::LedgerEvent;

fn task(key: &str, repo: Option<&str>, crate_name: Option<&str>) -> TaskNode {
    let number = natural_key(key).1 as u128;
    TaskNode {
        id: Uuid::from_u128(number),
        ticket_key: key.to_string(),
        slug: key.to_lowercase(),
        title: key.to_string(),
        description: String::new(),
        kind: TaskKind::Feature,
        status: TaskStatus::Ready,
        priority: 3,
        scope: TaskScope {
            repo: repo.map(str::to_string),
            crate_name: crate_name.map(str::to_string),
            ..TaskScope::default()
        },
        edges: Vec::new(),
        decisions: Vec::new(),
        acceptance_criteria: Vec::new(),
        plan: None,
        active_claim: None,
        latest_handoff: None,
        verification_receipt: None,
        created_at_epoch_ms: 1,
        updated_at_epoch_ms: 1,
        archived_at_epoch_ms: None,
    }
}

fn state_of(tasks: Vec<TaskNode>) -> LoadedState {
    let events: Vec<LedgerEvent> = tasks
        .into_iter()
        .map(|task| LedgerEvent::TaskCreated { task })
        .collect();
    LoadedState {
        layout: crate::storage_mode::StorageMode::Ledger,
        tasks: replay_events(&events),
        events,
    }
}

fn members() -> Vec<String> {
    vec![
        "benchwork".to_string(),
        "storypole".to_string(),
        "miter".to_string(),
    ]
}

fn owner(crate_name: &str) -> Option<String> {
    match crate_name {
        "benchwork-core" => Some("benchwork".to_string()),
        "storypole-core" => Some("storypole".to_string()),
        _ => None,
    }
}

fn destination_of(plan: &Plan, key: &str) -> Destination {
    plan.assignments
        .iter()
        .find(|assignment| assignment.ticket_key == key)
        .map(|assignment| assignment.destination.clone())
        .unwrap()
}

#[test]
fn a_scoped_ticket_goes_to_the_repository_its_scope_names() {
    let plan = plan(
        &state_of(vec![task("GB-2", Some("benchwork"), None)]),
        &members(),
        &owner,
    );

    assert_eq!(
        plan.assignments[0].destination,
        Destination::Member("benchwork".to_string())
    );
    assert_eq!(plan.assignments[0].evidence, "scope");
}

#[test]
fn a_suite_scope_stays_at_the_root() {
    let plan = plan(
        &state_of(vec![
            task("GB-3", Some("gaugeblock"), None),
            task("GB-4", Some("workspace"), None),
        ]),
        &members(),
        &owner,
    );

    assert_eq!(plan.staying(), 2);
    assert!(plan.assignments.iter().all(|a| a.evidence == "suite"));
}

#[test]
fn an_unscoped_ticket_is_adopted_only_when_its_crate_names_an_owner() {
    let plan = plan(
        &state_of(vec![
            task("GB-5", None, Some("benchwork-core")),
            task("GB-6", None, Some("some-unknown-crate")),
            task("GB-7", None, None),
        ]),
        &members(),
        &owner,
    );

    assert_eq!(
        destination_of(&plan, "GB-5"),
        Destination::Member("benchwork".to_string())
    );
    assert_eq!(
        destination_of(&plan, "GB-6"),
        Destination::Root,
        "no evidence, no move"
    );
    assert_eq!(
        destination_of(&plan, "GB-7"),
        Destination::Root,
        "no crate, no move"
    );
    assert_eq!(plan.adopted(), 1);
}

#[test]
fn a_suite_pseudo_crate_is_not_an_owner_even_when_the_lookup_offers_one() {
    // `workspace` names no repository, so a mapping that answered anyway would visibly file suite work in a
    // member. The filter has to be on the crate, not on the lookup's willingness.
    let plan = plan(
        &state_of(vec![task("GB-8", None, Some("workspace"))]),
        &members(),
        &|_| Some("benchwork".to_string()),
    );

    assert_eq!(plan.assignments[0].destination, Destination::Root);
    assert_eq!(plan.assignments[0].evidence, "unscoped");
}

#[test]
fn a_repository_no_member_claims_stays_at_the_root() {
    let plan = plan(
        &state_of(vec![task("GB-9", Some("ghost"), None)]),
        &members(),
        &owner,
    );

    assert_eq!(destination_of(&plan, "GB-9"), Destination::Root);
}

#[test]
fn the_plan_is_ordered_by_key_number_not_by_string() {
    let plan = plan(
        &state_of(vec![
            task("GB-10", Some("benchwork"), None),
            task("GB-9", Some("benchwork"), None),
        ]),
        &members(),
        &owner,
    );

    let keys: Vec<&str> = plan
        .assignments
        .iter()
        .map(|a| a.ticket_key.as_str())
        .collect();
    assert_eq!(keys, vec!["GB-9", "GB-10"]);
}

#[test]
fn the_counts_add_up_to_the_ticket_total() {
    let plan = plan(
        &state_of(vec![
            task("GB-1", Some("benchwork"), None),
            task("GB-2", Some("storypole"), None),
            task("GB-3", Some("gaugeblock"), None),
            task("GB-4", None, Some("miter-core")),
        ]),
        &members(),
        &owner,
    );

    let moved: usize = plan.per_member().values().sum();
    assert_eq!(moved + plan.staying(), plan.assignments.len());
    assert_eq!(moved, 2, "one by scope into benchwork, none by crate");
}

#[test]
fn the_restricted_plan_moves_only_what_the_ticket_itself_says() {
    // Adoption is a judgement about an old ticket; a scope move is not a judgement at all. Being able to apply
    // the second without the first is what keeps the risk proportional to the evidence.
    let plan = plan(
        &state_of(vec![
            task("GB-1", Some("benchwork"), None),
            task("GB-2", None, Some("benchwork-core")),
            task("GB-3", Some("gaugeblock"), None),
        ]),
        &members(),
        &owner,
    );
    assert_eq!(
        plan.per_member().values().sum::<usize>(),
        2,
        "both would move"
    );

    let restricted = plan.scope_only();

    assert_eq!(
        restricted.per_member().values().sum::<usize>(),
        1,
        "only the scoped ticket moves"
    );
    assert_eq!(
        destination_of(&restricted, "GB-2"),
        Destination::Root,
        "the adopted ticket waits for review"
    );
    assert_eq!(
        restricted
            .assignments
            .iter()
            .find(|a| a.ticket_key == "GB-2")
            .map(|a| a.evidence)
            .unwrap(),
        "crate",
        "and the restricted plan still says why it was a candidate"
    );
}
