//! Model Context Protocol (MCP) stdio server for `storypole`.

#![deny(unsafe_code)]

pub mod dispatch;
pub mod dispatch_helpers;
pub mod schema;
pub mod schema_plan;

use gaugeblock_mcp::{McpHandler, ToolOutcome};
use schema::list_tools;
use serde_json::Value;
use std::cell::RefCell;
use std::io::{self};
use std::path::Path;
use storypole_core::store::StorypoleStore;

/// MCP server version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// `storypole`'s tool surface, owning the store the tools mutate.
///
/// The store sits behind a `RefCell` because tool dispatch needs `&mut` and the handler trait hands out
/// `&self`. One process, one thread, one store - the borrow cannot overlap, because a call finishes
/// before the next line is read.
#[derive(Debug)]
pub struct StorypoleHandler {
    store: RefCell<StorypoleStore>,
}

impl StorypoleHandler {
    /// Opens the store at or above `root_dir`.
    ///
    /// # Errors
    ///
    /// Propagates the store's failure to open.
    pub fn open(root_dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            store: RefCell::new(StorypoleStore::open(root_dir)?),
        })
    }
}

impl McpHandler for StorypoleHandler {
    fn server_name(&self) -> &str {
        "storypole-mcp"
    }

    fn list_tools(&self) -> Value {
        Value::Array(list_tools())
    }

    fn call_tool(&self, name: &str, arguments: &Value) -> ToolOutcome {
        let mut store = self.store.borrow_mut();
        match dispatch::dispatch_tool(name, arguments, &mut store) {
            Ok(value) => {
                ToolOutcome::Text(serde_json::to_string_pretty(&value).unwrap_or_default())
            }
            Err(error) => ToolOutcome::Failed(error.to_string()),
        }
    }
}

/// Run the stdio JSON-RPC 2.0 MCP server loop over stdin/stdout.
///
/// # Errors
///
/// Returns the failure to open the store, or the I/O failure on stdin or stdout.
pub fn run_stdio_server(root_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let handler = StorypoleHandler::open(root_dir)?;
    tracing::info!("Starting storypole-mcp stdio server loop");
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    gaugeblock_mcp::serve(stdin.lock(), &mut stdout, &handler)?;
    Ok(())
}
