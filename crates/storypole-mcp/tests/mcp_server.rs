//! Integration test for `storypole-mcp` tools and dispatch.

use serde_json::json;
use storypole_core::config::StorypoleConfig;
use storypole_core::store::StorypoleStore;
use storypole_mcp::dispatch::dispatch_tool;
use storypole_mcp::schema::list_tools;

#[test]
fn test_mcp_tools_list() {
    let tools = list_tools();
    assert!(tools.len() >= 14);
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
        .collect();
    assert!(names.contains(&"storypole_next"));
    assert!(names.contains(&"storypole_add"));
    assert!(names.contains(&"storypole_claim"));
    assert!(names.contains(&"storypole_defer"));
    assert!(names.contains(&"storypole_ask_decision"));
}

#[test]
fn test_mcp_tool_dispatch_lifecycle() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut store = StorypoleStore::init(tmp.path(), StorypoleConfig::default()).expect("init");

    // 1. Add task via MCP tool
    let add_args = json!({
        "title": "MCP Integrated Task",
        "description": "Created via MCP dispatch",
        "kind": "feature",
        "priority": 1,
        "repo": "gaugeblock",
        "crate_name": "storypole-mcp",
        "labels": ["mcp", "integration"]
    });
    let add_res = dispatch_tool("storypole_add", &add_args, &mut store).expect("add");
    let ticket_key = add_res
        .get("ticket_key")
        .and_then(|k| k.as_str())
        .expect("ticket_key");
    assert_eq!(ticket_key, "SP-1");

    // 2. Query next task
    let next_args = json!({ "repo": "gaugeblock" });
    let next_res = dispatch_tool("storypole_next", &next_args, &mut store).expect("next");
    assert_eq!(
        next_res
            .get("task")
            .and_then(|t| t.get("ticket_key"))
            .and_then(|k| k.as_str()),
        Some("SP-1")
    );

    // 3. Claim task
    let claim_args = json!({
        "identifier": "SP-1",
        "agent_id": "test-agent"
    });
    let claim_res = dispatch_tool("storypole_claim", &claim_args, &mut store).expect("claim");
    assert_eq!(
        claim_res
            .get("task")
            .and_then(|t| t.get("status"))
            .and_then(|s| s.as_str()),
        Some("in_progress")
    );

    // 4. Defer issue
    let defer_args = json!({
        "title": "Deferred AST anomaly",
        "kind": "bug",
        "captured_by": "test-agent",
        "discovered_from": "SP-1"
    });
    let defer_res = dispatch_tool("storypole_defer", &defer_args, &mut store).expect("defer");
    assert_eq!(
        defer_res.get("status").and_then(|s| s.as_str()),
        Some("triage")
    );
}
