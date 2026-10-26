//! Integration tests for multi-repository workspace discovery, unified rollups, and cross-repo edges.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use storypole_core::config::StorypoleConfig;
use storypole_core::index::query::{TaskFilter, query_tasks};
use storypole_core::model::task::{EdgeKind, TaskEdge, TaskScope};
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::store::StorypoleStore;
use storypole_core::workspace::store::StorypoleWorkspaceStore;

#[test]
fn test_workspace_discovery_and_unified_rollup() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ws_root = tmp.path();

    // 1. Initialize root store
    let root_cfg = StorypoleConfig {
        ticket_prefix: "GB".into(),
        claim_ttl_minutes: 60,
        ..Default::default()
    };
    let mut root_store = StorypoleStore::init(ws_root, root_cfg).expect("init root");
    let root_task = add_task(
        &mut root_store,
        CreateTaskParams {
            title: "Meta Integration".into(),
            scope: TaskScope {
                repo: Some("workspace".into()),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("root task");
    assert_eq!(root_task.ticket_key, "GB-1");

    // 2. Initialize benchwork subrepo
    let bw_dir = ws_root.join("benchwork");
    fs::create_dir_all(&bw_dir).expect("create bw dir");
    let bw_cfg = StorypoleConfig {
        ticket_prefix: "BW".into(),
        claim_ttl_minutes: 60,
        ..Default::default()
    };
    let mut bw_store = StorypoleStore::init(&bw_dir, bw_cfg).expect("init bw");
    let bw_task = add_task(
        &mut bw_store,
        CreateTaskParams {
            title: "Workbench UI".into(),
            scope: TaskScope {
                repo: Some("benchwork".into()),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("bw task");
    assert_eq!(bw_task.ticket_key, "BW-1");

    // 3. Initialize miter subrepo
    let mtr_dir = ws_root.join("miter");
    fs::create_dir_all(&mtr_dir).expect("create mtr dir");
    let mtr_cfg = StorypoleConfig {
        ticket_prefix: "MTR".into(),
        claim_ttl_minutes: 60,
        ..Default::default()
    };
    let mut mtr_store = StorypoleStore::init(&mtr_dir, mtr_cfg).expect("init mtr");
    let mtr_task = add_task(
        &mut mtr_store,
        CreateTaskParams {
            title: "Tree Sitter Indexer".into(),
            scope: TaskScope {
                repo: Some("miter".into()),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("mtr task");
    assert_eq!(mtr_task.ticket_key, "MTR-1");

    // 4. Open workspace store and check rollup
    let ws = StorypoleWorkspaceStore::open(ws_root).expect("open ws");
    assert_eq!(ws.members().len(), 2);

    let all_tasks = query_tasks(ws.conn(), &TaskFilter::default()).expect("query all");
    assert_eq!(all_tasks.len(), 3);

    // Filter by repo
    let bw_tasks = query_tasks(
        ws.conn(),
        &TaskFilter {
            repo: Some("benchwork".into()),
            ..Default::default()
        },
    )
    .expect("query bw");
    assert_eq!(bw_tasks.len(), 1);
    assert_eq!(bw_tasks[0].ticket_key, "BW-1");

    // Check governing store routing
    assert_eq!(
        ws.resolve_governing_store_name("BW-1"),
        Some("benchwork".into())
    );
    assert_eq!(
        ws.resolve_governing_store_name("MTR-1"),
        Some("miter".into())
    );
}

#[test]
fn test_cross_repo_edge_metadata() {
    let target_id = uuid::Uuid::new_v4();
    let edge = TaskEdge::new(EdgeKind::BlockedBy, target_id)
        .with_target_meta("MTR-12", Some("miter"));

    assert_eq!(edge.target_ticket_key.as_deref(), Some("MTR-12"));
    assert_eq!(edge.target_repo.as_deref(), Some("miter"));

    // Verify serde round-trip
    let json = serde_json::to_string(&edge).expect("to_string");
    assert!(json.contains("\"target_ticket_key\":\"MTR-12\""));
    assert!(json.contains("\"target_repo\":\"miter\""));

    let deserialized: TaskEdge = serde_json::from_str(&json).expect("from_str");
    assert_eq!(deserialized, edge);
}

