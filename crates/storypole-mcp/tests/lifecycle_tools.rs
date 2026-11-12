//! The lifecycle transitions are reachable over MCP, not only from a shell (`SP-52`).
//!
//! The defect these pin: `park`, `unpark` and `reopen` existed in the library and on the command line, and
//! `storypole-mcp` exposed none of them - so an agent working through the server had to drop to a shell to
//! hold a ticket, and the call it made then was invisible to the trace.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::{Value, json};
use storypole_core::StorypoleConfig;
use storypole_core::store::StorypoleStore;
use storypole_mcp::dispatch::dispatch_tool;
use storypole_mcp::schema::list_tools;

#[test]
fn the_three_transitions_are_advertised() {
    let names: Vec<String> = list_tools()
        .iter()
        .filter_map(|tool| tool.get("name").and_then(|name| name.as_str()))
        .map(str::to_string)
        .collect();
    for tool in ["storypole_park", "storypole_unpark", "storypole_reopen"] {
        assert!(names.iter().any(|name| name == tool), "{tool} is listed");
    }
}

#[test]
fn a_round_trip_parks_and_unparks_one_task() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut store = StorypoleStore::init(tmp.path(), StorypoleConfig::default()).expect("init");

    let key = add(&mut store, "Hold this");

    let parked = call(
        &mut store,
        "storypole_park",
        json!({"identifier": key, "reason": "owner hold"}),
    );
    assert_eq!(parked["status"], "Deferred");

    let unparked = call(&mut store, "storypole_unpark", json!({"identifier": key}));
    assert_eq!(unparked["status"], "Ready");
}

#[test]
fn reopening_over_mcp_requires_a_reason() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut store = StorypoleStore::init(tmp.path(), StorypoleConfig::default()).expect("init");
    let key = add(&mut store, "Never verified");

    // The reason is part of the contract, not a convenience: reopening withdraws a certification.
    let refused = dispatch_tool("storypole_reopen", &json!({"identifier": key}), &mut store);
    assert!(refused.is_err(), "a reopen without a reason is refused");
}

/// Adds one task and returns its key.
fn add(store: &mut StorypoleStore, title: &str) -> String {
    let added = call(
        store,
        "storypole_add",
        json!({"title": title, "kind": "feature"}),
    );
    added["ticket_key"].as_str().expect("key").to_string()
}

/// One MCP tool call, asserting it succeeded.
fn call(store: &mut StorypoleStore, tool: &str, args: Value) -> Value {
    dispatch_tool(tool, &args, store).unwrap_or_else(|error| panic!("{tool} failed: {error:?}"))
}
