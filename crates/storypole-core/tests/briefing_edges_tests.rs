//! Integration tests for task briefing link summarisation in both edge directions.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use storypole_core::config::StorypoleConfig;
use storypole_core::dag::briefing::build_task_briefing;
use storypole_core::index::query::get_all_tasks;
use storypole_core::model::task::{EdgeKind, TaskKind, TaskStatus};
use storypole_core::ops::graph::link_tasks;
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::store::StorypoleStore;

fn temp_store() -> StorypoleStore {
    let tmp = tempfile::tempdir().expect("tempdir");
    StorypoleStore::init(tmp.path(), StorypoleConfig::default()).expect("init")
}

fn add(store: &mut StorypoleStore, title: &str) -> storypole_core::model::task::TaskNode {
    add_task(
        store,
        CreateTaskParams {
            title: title.into(),
            ..Default::default()
        },
    )
    .expect("add task")
}

fn related_keys(store: &StorypoleStore, id: uuid::Uuid) -> Vec<String> {
    let mut all = get_all_tasks(store.conn()).expect("all tasks");
    let task = all.remove(&id).expect("task present");
    let briefing = build_task_briefing(&task, &all, store.config(), StorypoleStore::now_ms());
    briefing
        .related_tasks_summary
        .into_iter()
        .map(|(key, _title, _status)| key)
        .collect()
}

#[test]
fn test_epic_briefing_lists_its_child_subtasks() {
    let mut store = temp_store();
    let epic = add(&mut store, "Phase epic");
    let first = add(&mut store, "First child");
    let second = add(&mut store, "Second child");

    link_tasks(&mut store, first.id, epic.id, EdgeKind::ParentEpic).expect("link first");
    link_tasks(&mut store, second.id, epic.id, EdgeKind::ParentEpic).expect("link second");

    let epic_links = related_keys(&store, epic.id);
    assert_eq!(
        epic_links.len(),
        2,
        "an epic must surface both children, got {epic_links:?}"
    );
    assert!(epic_links.contains(&first.ticket_key));
    assert!(epic_links.contains(&second.ticket_key));

    let child_links = related_keys(&store, first.id);
    assert_eq!(
        child_links,
        vec![epic.ticket_key.clone()],
        "a child must still surface its parent epic"
    );
}

#[test]
fn test_blocked_by_edges_surface_in_both_directions() {
    let mut store = temp_store();
    let blocker = add(&mut store, "Upstream dependency");
    let dependent = add(&mut store, "Dependent task");

    link_tasks(&mut store, dependent.id, blocker.id, EdgeKind::BlockedBy).expect("link");

    assert_eq!(
        related_keys(&store, dependent.id),
        vec![blocker.ticket_key.clone()],
        "the dependent must list what blocks it"
    );
    assert_eq!(
        related_keys(&store, blocker.id),
        vec![dependent.ticket_key.clone()],
        "the blocker must list what it is holding back"
    );
}

#[test]
fn test_mutual_related_to_links_are_not_duplicated() {
    let mut store = temp_store();
    let left = add(&mut store, "Left task");
    let right = add(&mut store, "Right task");

    link_tasks(&mut store, left.id, right.id, EdgeKind::RelatedTo).expect("left -> right");
    link_tasks(&mut store, right.id, left.id, EdgeKind::RelatedTo).expect("right -> left");

    for (task, other) in [(&left, &right), (&right, &left)] {
        let links = related_keys(&store, task.id);
        assert_eq!(
            links,
            vec![other.ticket_key.clone()],
            "mutual links must collapse to a single entry"
        );
    }
}

#[test]
fn test_parent_epic_decisions_are_inherited_by_children() {
    let mut store = temp_store();
    let epic = add(&mut store, "Epic with decision");
    let child = add(&mut store, "Child");

    link_tasks(&mut store, child.id, epic.id, EdgeKind::ParentEpic).expect("link child");

    let mut all = get_all_tasks(store.conn()).expect("all tasks");
    let task = all.remove(&child.id).expect("child present");
    let briefing = build_task_briefing(&task, &all, store.config(), StorypoleStore::now_ms());

    // The parent epic is surfaced even when it carries no decisions yet.
    assert!(
        briefing
            .related_tasks_summary
            .iter()
            .any(|(key, _, status)| key == &epic.ticket_key && *status == TaskStatus::Ready),
        "child briefing must reference the ready parent epic"
    );
    assert_eq!(task.kind, TaskKind::Feature);
}
