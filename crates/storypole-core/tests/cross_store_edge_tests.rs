//! A dependency that spans two repositories has to be recordable (`SP-48`).
//!
//! The defect these pin: `link` resolved both ends inside the store it ran in, so since the 2026-11-08
//! migration moved tickets into their owning repositories, no cross-repository edge could be created at
//! all. The seven `blocked_by`, fifteen `related_to` and twelve `parent_epic` cross-ledger edges in the
//! suite are historical; every new one failed with `Task not found`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use storypole_core::StorypoleConfig;
use storypole_core::model::task::{EdgeKind, TaskStatus};
use storypole_core::ops::graph::{EdgeTarget, link_to_target};
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::store::StorypoleStore;
use storypole_core::workspace::find_task;
use tempfile::tempdir;

/// A suite declaring one member, both in the suite layout and both records mode.
fn suite_with_member(root: &Path) -> (StorypoleStore, StorypoleStore) {
    fs::create_dir_all(root.join(".gaugeblock").join("storypole").join("tickets"))
        .expect("suite store dir");
    let suite = StorypoleStore::init(root, StorypoleConfig::default()).expect("init suite");
    fs::remove_dir_all(root.join(".storypole")).ok();
    fs::write(
        root.join(".gaugeblock").join("config.toml"),
        "[suite]\nmembers = [\"member-a\"]\n",
    )
    .expect("suite declaration");

    let member_root = root.join("member-a");
    fs::create_dir_all(member_root.join(".git")).expect("member checkout");
    fs::create_dir_all(
        member_root
            .join(".gaugeblock")
            .join("storypole")
            .join("tickets"),
    )
    .expect("member store dir");
    let config = StorypoleConfig {
        ticket_prefix: "MA".to_string(),
        ..StorypoleConfig::default()
    };
    let member = StorypoleStore::init(&member_root, config).expect("init member");
    fs::remove_dir_all(member_root.join(".storypole")).ok();
    (suite, member)
}

#[test]
fn a_task_in_another_repository_is_findable_and_linkable() {
    let tmp = tempdir().expect("tempdir");
    let (mut suite, mut member) = suite_with_member(tmp.path());

    let held = add_task(
        &mut suite,
        CreateTaskParams {
            title: "Suite work waiting on a member".into(),
            ..Default::default()
        },
    )
    .expect("suite task");
    let prerequisite = add_task(
        &mut member,
        CreateTaskParams {
            title: "Member work that has to land first".into(),
            ..Default::default()
        },
    )
    .expect("member task");

    // The target is read from the store that owns it, not copied into this one.
    let found = find_task(tmp.path(), &prerequisite.ticket_key)
        .expect("lookup")
        .expect("the member's task is findable from the suite");
    assert_eq!(found.task.id, prerequisite.id);
    assert_eq!(found.repo.as_deref(), Some("member-a"));

    let target = EdgeTarget {
        id: found.task.id,
        key: found.task.ticket_key.clone(),
        repo: found.repo,
        verified: found.task.status == TaskStatus::Verified,
    };
    link_to_target(&mut suite, held.id, &target, EdgeKind::BlockedBy).expect("link across stores");

    let stored = storypole_core::index::query::get_task_by_id(suite.conn(), held.id).expect("read");
    let edge = stored
        .edges
        .iter()
        .find(|edge| edge.target_id == prerequisite.id)
        .expect("the edge is on the source");
    assert_eq!(edge.kind, EdgeKind::BlockedBy);
    assert_eq!(
        edge.target_ticket_key.as_deref(),
        Some(prerequisite.ticket_key.as_str()),
        "the locator renders without opening the other repository"
    );
    assert_eq!(edge.target_repo.as_deref(), Some("member-a"));
    assert_eq!(
        stored.status,
        TaskStatus::Blocked,
        "an unverified prerequisite blocks the source"
    );

    // No copy of the target was written here - that is how GB-79 diverged.
    assert!(
        storypole_core::index::query::get_all_tasks(suite.conn())
            .expect("read suite")
            .len()
            == 1,
        "the suite store still holds exactly its own one ticket"
    );
}

#[test]
fn a_key_no_store_holds_is_reported_as_missing() {
    let tmp = tempdir().expect("tempdir");
    let (_suite, _member) = suite_with_member(tmp.path());

    assert!(
        find_task(tmp.path(), "ZZ-404").expect("lookup").is_none(),
        "an unknown key stays unknown rather than inventing a location"
    );
}
