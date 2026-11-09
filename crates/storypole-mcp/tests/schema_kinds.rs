//! The MCP schema advertises exactly the kinds the contract accepts (`GB-343`).

#![allow(clippy::unwrap_used)]

use storypole_core::model::task::TaskKind;
use storypole_mcp::schema::list_tools;

/// The `kind` enum every tool advertises, as a client reads it.
fn advertised(tool: &str) -> Vec<String> {
    let tools = list_tools();
    let found = tools
        .iter()
        .find(|t| t.get("name").and_then(|n| n.as_str()) == Some(tool))
        .unwrap();
    found["inputSchema"]["properties"]["kind"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn the_advertised_kinds_are_exactly_the_contracts_kinds() {
    // A schema that advertises fewer kinds than the contract accepts fails invisibly: the client never
    // sends the value, so nothing ever reports it (`GB-343`).
    let expected: Vec<String> = TaskKind::ALL
        .iter()
        .map(|k| k.as_str().to_string())
        .collect();
    for tool in ["storypole_add", "storypole_defer"] {
        assert_eq!(
            advertised(tool),
            expected,
            "{tool} advertises the contract's kinds"
        );
    }
}
