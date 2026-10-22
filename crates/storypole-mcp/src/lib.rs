//! Model Context Protocol (MCP) stdio server for `storypole`.

#![deny(unsafe_code)]

pub mod dispatch;
pub mod dispatch_helpers;
pub mod schema;

use dispatch::dispatch_tool;
use schema::list_tools;
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use std::path::Path;
use storypole_core::store::StorypoleStore;

/// MCP server version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Run the stdio JSON-RPC 2.0 MCP server loop over stdin/stdout.
pub fn run_stdio_server(root_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = StorypoleStore::open(root_dir)?;
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    tracing::info!("Starting storypole-mcp stdio server loop");

    for line_res in stdin.lock().lines() {
        let line = match line_res {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("Failed to read stdin line: {e}");
                break;
            }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let msg: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("Failed to parse JSON-RPC message: {e}");
                continue;
            }
        };

        if let Some(resp) = handle_message(&msg, &mut store) {
            let resp_str = serde_json::to_string(&resp)?;
            writeln!(stdout, "{resp_str}")?;
            stdout.flush()?;
        }
    }

    Ok(())
}

fn handle_message(msg: &Value, store: &mut StorypoleStore) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(|m| m.as_str())?;

    match method {
        "initialize" => {
            let resp = json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "storypole-mcp",
                        "version": VERSION
                    }
                }
            });
            Some(resp)
        }
        "notifications/initialized" => None,
        "ping" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {}
        })),
        "tools/list" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": list_tools()
            }
        })),
        "tools/call" => {
            let params = msg.get("params")?;
            let name = params.get("name").and_then(|n| n.as_str())?;
            let default_args = json!({});
            let args = params.get("arguments").unwrap_or(&default_args);

            let (text, is_error) = match dispatch_tool(name, args, store) {
                Ok(val) => (
                    serde_json::to_string_pretty(&val).unwrap_or_default(),
                    false,
                ),
                Err(err) => (format!("Error: {err}"), true),
            };

            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [
                        {
                            "type": "text",
                            "text": text
                        }
                    ],
                    "isError": is_error
                }
            }))
        }
        other => {
            tracing::warn!("Unsupported method: {other}");
            id.map(|req_id| {
                json!({
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "error": {
                        "code": -32601,
                        "message": format!("Method not found: {other}")
                    }
                })
            })
        }
    }
}
