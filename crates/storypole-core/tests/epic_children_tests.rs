//! An epic cannot be verified while its children are open (`GB-334`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use gaugeblock_contracts::TaskStatus;
use gaugeblock_contracts::storypole::{EdgeKind, TaskKind, TaskScope};
use storypole_core::config::StorypoleConfig;
use storypole_core::ops::graph::link_tasks;
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::ops::verification::verify_task;
use storypole_core::store::StorypoleStore;
use tempfile::TempDir;
use uuid::Uuid;

fn add(store: &mut StorypoleStore, title: &str, kind: TaskKind) -> Uuid {
    add_task(
        store,
        CreateTaskParams {
            title: title.to_string(),
            description: String::new(),
            kind,
            scope: TaskScope::default(),
            ..CreateTaskParams::default()
        },
    )
    .expect("add")
    .id
}

/// Makes `child` name `epic` as its parent, through the same op `storypole link` uses.
fn adopt(store: &mut StorypoleStore, child: Uuid, epic: Uuid) {
    link_tasks(store, child, epic, EdgeKind::ParentEpic).expect("link child to epic");
}

#[test]
fn an_epic_with_an_open_child_is_refused() {
    let temp = TempDir::new().unwrap();
    let mut store = StorypoleStore::init(temp.path(), StorypoleConfig::default()).expect("init");
    let epic = add(&mut store, "an epic", TaskKind::Epic);
    let child = add(&mut store, "a child", TaskKind::Feature);
    adopt(&mut store, child, epic);

    let error = verify_task(&mut store, epic, None, false, false).expect_err("a child is open");

    let message = error.to_string();
    assert!(message.contains("unfinished child"), "{message}");
    assert!(message.contains("SP-"), "and names it: {message}");
}

#[test]
fn an_epic_whose_children_are_verified_can_be_closed() {
    let temp = TempDir::new().unwrap();
    let mut store = StorypoleStore::init(temp.path(), StorypoleConfig::default()).expect("init");
    let epic = add(&mut store, "an epic", TaskKind::Epic);
    let child = add(&mut store, "a child", TaskKind::Feature);
    adopt(&mut store, child, epic);

    verify_task(&mut store, child, None, false, false).expect("child first");
    verify_task(&mut store, epic, None, false, false).expect("then the epic");
}

#[test]
fn a_non_epic_task_with_children_is_not_refused() {
    // Featuring a child under a feature is a legitimate shape; the rule is about what an epic claims.
    let temp = TempDir::new().unwrap();
    let mut store = StorypoleStore::init(temp.path(), StorypoleConfig::default()).expect("init");
    let parent = add(&mut store, "a feature", TaskKind::Feature);
    let child = add(&mut store, "a child", TaskKind::Feature);
    adopt(&mut store, child, parent);

    verify_task(&mut store, parent, None, false, false)
        .expect("a feature may close with open children");
}

#[test]
fn a_child_that_is_still_ready_reports_its_status_in_the_message() {
    let temp = TempDir::new().unwrap();
    let mut store = StorypoleStore::init(temp.path(), StorypoleConfig::default()).expect("init");
    let epic = add(&mut store, "an epic", TaskKind::Epic);
    let child = add(&mut store, "a child", TaskKind::Feature);
    adopt(&mut store, child, epic);

    let error = verify_task(&mut store, epic, None, false, false).expect_err("open child");
    let message = error.to_string();

    assert!(
        message.contains("Ready"),
        "the child's state is the actionable part: {message}"
    );
    let _ = TaskStatus::Ready;
}
