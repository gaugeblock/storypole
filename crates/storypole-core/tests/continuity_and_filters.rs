//! Integration tests for agent continuity, handoff packets, collision detection, and multi-dimensional filters.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use storypole_core::config::StorypoleConfig;
use storypole_core::index::query::{TaskFilter, query_tasks, resolve_task_id};
use storypole_core::model::decision::HandoffPacket;
use storypole_core::model::task::{EdgeKind, TaskEdge, TaskKind, TaskScope, TaskStatus};
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::ops::workflow::{claim_task, handoff_task, next_task, verify_task};
use storypole_core::store::StorypoleStore;

fn temp_store() -> StorypoleStore {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = StorypoleConfig {
        claim_ttl_minutes: 60,
        ..Default::default()
    };
    StorypoleStore::init(tmp.path(), config).expect("init")
}

#[test]
fn test_agent_claim_and_collision_briefing() {
    let mut store = temp_store();

    let task1 = add_task(
        &mut store,
        CreateTaskParams {
            title: "Task 1 File Claim".into(),
            scope: TaskScope {
                repo: Some("gaugeblock".into()),
                crate_name: Some("storypole-core".into()),
                target_files: vec!["crates/storypole-core/src/lib.rs".into()],
                target_symbols: vec!["storypole_core::StorypoleStore".into()],
                labels: vec!["core".into(), "urgent".into()],
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("task1");

    let task2 = add_task(
        &mut store,
        CreateTaskParams {
            title: "Task 2 Collision Target".into(),
            scope: TaskScope {
                repo: Some("gaugeblock".into()),
                crate_name: Some("storypole-core".into()),
                target_files: vec!["crates/storypole-core/src/lib.rs".into()],
                labels: vec!["core".into()],
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("task2");

    // Agent 1 claims Task 1
    let briefing1 = claim_task(&mut store, task1.id, "agent-1", None).expect("claim1");
    assert_eq!(briefing1.task.status, TaskStatus::InProgress);
    assert_eq!(
        briefing1.task.active_claim.as_ref().map(|c| &c.agent_id),
        Some(&"agent-1".to_string())
    );

    // Agent 2 attempts claim on Task 1 -> Conflict!
    let conflict = claim_task(&mut store, task1.id, "agent-2", None);
    assert!(conflict.is_err(), "Expected ClaimConflict");

    // Next query with exclude_file_collisions: true should avoid task2
    let filter = TaskFilter {
        exclude_file_collisions: true,
        ..Default::default()
    };
    let next_opt = next_task(&mut store, &filter, None).expect("next");
    assert!(
        next_opt.is_none(),
        "Expected no ready task due to file collision exclusion"
    );

    // Without collision exclusion, task2 is returned with collision warning in briefing
    let briefing2 = next_task(&mut store, &TaskFilter::default(), None)
        .expect("next")
        .expect("briefing2");
    assert_eq!(briefing2.task.id, task2.id);
    assert!(
        !briefing2.file_collision_warnings.is_empty(),
        "Expected collision warning on target_files"
    );
}

#[test]
fn test_handoff_packet_persistence() {
    let mut store = temp_store();

    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Handoff Task".into(),
            ..Default::default()
        },
    )
    .expect("task");

    claim_task(&mut store, task.id, "worker-1", None).expect("claim");

    let packet = HandoffPacket {
        recorded_by_agent: "worker-1".into(),
        timestamp_ms: StorypoleStore::now_ms(),
        completed_items: vec!["Implemented schema".into(), "Added events".into()],
        remaining_items: vec!["Add test cases".into()],
        last_error_or_blocker: Some("Need clarification on CLI flags".into()),
        git_ref: Some("feat/step1".into()),
        dirty_files: vec!["src/lib.rs".into()],
    };

    handoff_task(&mut store, task.id, packet.clone(), TaskStatus::Ready).expect("handoff");

    let reloaded =
        storypole_core::index::query::get_task_by_id(store.conn(), task.id).expect("reload");
    assert_eq!(reloaded.status, TaskStatus::Ready);
    assert!(
        reloaded.active_claim.is_none(),
        "Claim must be released on handoff"
    );
    assert_eq!(reloaded.latest_handoff, Some(packet));
}

#[test]
fn test_multi_dimensional_filters_and_dual_id_resolution() {
    let mut store = temp_store();

    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Filter and Resolution Test".into(),
            kind: TaskKind::Bug,
            priority: 2,
            scope: TaskScope {
                repo: Some("gaugeblock".into()),
                crate_name: Some("storypole-mcp".into()),
                module_path: Some("server::stdio".into()),
                target_files: vec!["crates/storypole-mcp/src/lib.rs".into()],
                target_symbols: vec!["run_stdio_server".into()],
                labels: vec!["mcp".into(), "p0".into()],
            },
            ..Default::default()
        },
    )
    .expect("task");

    // Dual ID resolution:
    // 1. By ticket_key lowercase
    let resolved_key = resolve_task_id(store.conn(), &task.ticket_key.to_lowercase()).expect("key");
    assert_eq!(resolved_key, task.id);

    // 2. By slug
    let resolved_slug = resolve_task_id(store.conn(), &task.slug).expect("slug");
    assert_eq!(resolved_slug, task.id);

    // 3. By UUID prefix
    let prefix = &task.id.to_string()[..8];
    let resolved_prefix = resolve_task_id(store.conn(), prefix).expect("prefix");
    assert_eq!(resolved_prefix, task.id);

    // Filter by repo & crate_name
    let res = query_tasks(
        store.conn(),
        &TaskFilter {
            repo: Some("gaugeblock".into()),
            crate_name: Some("storypole-mcp".into()),
            ..Default::default()
        },
    )
    .expect("filter");
    assert_eq!(res.len(), 1);

    // Filter by label
    let res_label = query_tasks(
        store.conn(),
        &TaskFilter {
            label: Some("mcp".into()),
            ..Default::default()
        },
    )
    .expect("filter label");
    assert_eq!(res_label.len(), 1);

    // Filter by target_file
    let res_file = query_tasks(
        store.conn(),
        &TaskFilter {
            target_file: Some("crates/storypole-mcp/src/lib.rs".into()),
            ..Default::default()
        },
    )
    .expect("filter file");
    assert_eq!(res_file.len(), 1);
}

#[test]
fn test_verify_unblocks_downstream_task() {
    let mut store = temp_store();

    let parent = add_task(
        &mut store,
        CreateTaskParams {
            title: "Parent Dependency".into(),
            ..Default::default()
        },
    )
    .expect("parent");

    let child = add_task(
        &mut store,
        CreateTaskParams {
            title: "Child Worker".into(),
            edges: vec![TaskEdge::new(EdgeKind::BlockedBy, parent.id)],
            ..Default::default()
        },
    )
    .expect("child");

    assert_eq!(child.status, TaskStatus::Blocked);

    verify_task(&mut store, parent.id, None, false, false).expect("verify");

    let child_after =
        storypole_core::index::query::get_task_by_id(store.conn(), child.id).expect("get");
    assert_eq!(child_after.status, TaskStatus::Ready);
}
