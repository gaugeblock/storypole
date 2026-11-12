//! Lifecycle tool schemas for Storypole MCP (`SP-52`).
//!
//! Split out of `schema.rs`, which reached 278 lines when the three transition tools were added - the same
//! reason `schema_plan.rs` exists, and the same shape: a small module returning its own slice of the list.

use serde_json::{Value, json};

/// Return the lifecycle-transition MCP tool schemas.
pub fn lifecycle_tools() -> Vec<Value> {
    vec![
        crate::schema::tool(
            "storypole_park",
            "Park a task: it stays in the backlog and leaves the ready queue. Refuses a verified task, a held claim, or an archived one",
            json!({
                "type": "object",
                "properties": {
                    "identifier": { "type": "string" },
                    "reason": { "type": "string" }
                },
                "required": ["identifier"]
            }),
        ),
        crate::schema::tool(
            "storypole_unpark",
            "Return a parked task to the backlog, as blocked rather than ready when a dependency is unverified",
            json!({
                "type": "object",
                "properties": {
                    "identifier": { "type": "string" }
                },
                "required": ["identifier"]
            }),
        ),
        crate::schema::tool(
            "storypole_reopen",
            "Reopen a verified task, keeping the receipt that certified it. The reason is required: reopening withdraws a certification",
            json!({
                "type": "object",
                "properties": {
                    "identifier": { "type": "string" },
                    "reason": { "type": "string" }
                },
                "required": ["identifier", "reason"]
            }),
        ),
    ]
}
