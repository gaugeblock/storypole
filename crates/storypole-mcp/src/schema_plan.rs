//! Plan tool schemas for Storypole MCP.

use serde_json::{Value, json};

/// Return plan-related MCP tool schemas.
pub fn plan_tools() -> Vec<Value> {
    vec![crate::schema::tool(
        "storypole_set_plan",
        "Attach or update an execution plan and design markdown blueprint on a task",
        json!({
            "type": "object",
            "properties": {
                "identifier": { "type": "string" },
                "summary": { "type": "string" },
                "design_markdown": { "type": "string" },
                "steps": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "step_id": { "type": "string" },
                            "title": { "type": "string" },
                            "target_files": { "type": "array", "items": { "type": "string" } },
                            "verification_gate": { "type": "string" },
                            "completed": { "type": "boolean" }
                        },
                        "required": ["step_id", "title"]
                    }
                }
            },
            "required": ["identifier", "summary", "design_markdown"]
        }),
    )]
}
