//! MCP tool schemas and definitions for `storypole-mcp`.

use serde_json::{Value, json};

/// Enumerate all MCP tools exposed by the Storypole server.
pub fn list_tools() -> Vec<Value> {
    let mut tools = vec![
        tool(
            "storypole_next",
            "Retrieve next ready task matching filters, with context briefing and collision warnings",
            json!({
                "type": "object",
                "properties": {
                    "repo": { "type": "string" },
                    "crate_name": { "type": "string" },
                    "module_path": { "type": "string" },
                    "label": { "type": "string" },
                    "priority": { "type": "integer" },
                    "exclude_file_collisions": { "type": "boolean" },
                    "auto_claim_agent": { "type": "string" }
                }
            }),
        ),
        tool(
            "storypole_show",
            "Show comprehensive TaskContextBriefing by ticket key, slug, or UUID",
            json!({
                "type": "object",
                "properties": {
                    "identifier": { "type": "string" }
                },
                "required": ["identifier"]
            }),
        ),
        tool(
            "storypole_add",
            "Create a new task node in the backlog",
            json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "kind": { "type": "string", "enum": ["feature", "bug", "refactor", "tech_debt", "spike", "epic"] },
                    "priority": { "type": "integer", "minimum": 1, "maximum": 5 },
                    "repo": { "type": "string" },
                    "crate_name": { "type": "string" },
                    "target_files": { "type": "array", "items": { "type": "string" } },
                    "target_symbols": { "type": "array", "items": { "type": "string" } },
                    "labels": { "type": "array", "items": { "type": "string" } },
                    "acceptance_criteria": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["title"]
            }),
        ),
        tool(
            "storypole_update",
            "Update attributes on an existing task node",
            json!({
                "type": "object",
                "properties": {
                    "identifier": { "type": "string" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "priority": { "type": "integer" },
                    "repo": { "type": "string" },
                    "crate_name": { "type": "string" },
                    "target_files": { "type": "array", "items": { "type": "string" } },
                    "target_symbols": { "type": "array", "items": { "type": "string" } },
                    "labels": { "type": "array", "items": { "type": "string" } },
                    "acceptance_criteria": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["identifier"]
            }),
        ),
        tool(
            "storypole_link",
            "Connect two tasks with a directed edge (BlockedBy, RelatedTo, ParentEpic)",
            json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string" },
                    "target": { "type": "string" },
                    "kind": { "type": "string", "enum": ["blocked_by", "related_to", "parent_epic", "discovered_from"] }
                },
                "required": ["source", "target", "kind"]
            }),
        ),
        tool(
            "storypole_defer",
            "Capture an out-of-band issue or tech debt in <5ms without disrupting current task",
            json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "kind": { "type": "string", "enum": ["bug", "tech_debt", "spike", "feature"] },
                    "discovered_from": { "type": "string" },
                    "captured_by": { "type": "string" },
                    "repo": { "type": "string" },
                    "crate_name": { "type": "string" },
                    "target_files": { "type": "array", "items": { "type": "string" } },
                    "target_symbols": { "type": "array", "items": { "type": "string" } },
                    "labels": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["title", "captured_by"]
            }),
        ),
        tool(
            "storypole_split",
            "Split an oversized task into a parent Epic and sequential or parallel child tasks",
            json!({
                "type": "object",
                "properties": {
                    "identifier": { "type": "string" },
                    "child_titles": { "type": "array", "items": { "type": "string" } },
                    "sequential": { "type": "boolean" }
                },
                "required": ["identifier", "child_titles"]
            }),
        ),
        tool(
            "storypole_ask_decision",
            "Raise an architectural decision requirement, automatically blocking the task",
            json!({
                "type": "object",
                "properties": {
                    "identifier": { "type": "string" },
                    "category": { "type": "string" },
                    "question": { "type": "string" },
                    "proposed_options": { "type": "array", "items": { "type": "string" } },
                    "parent_decision_id": { "type": "string" }
                },
                "required": ["identifier", "question", "proposed_options"]
            }),
        ),
        tool(
            "storypole_decide",
            "Resolve a pending decision with chosen option and rationale, unblocking task if clear",
            json!({
                "type": "object",
                "properties": {
                    "identifier": { "type": "string" },
                    "decision_id": { "type": "string" },
                    "chosen_option": { "type": "string" },
                    "rationale": { "type": "string" },
                    "resolved_by": { "type": "string" },
                    "appended_criterion": { "type": "string" }
                },
                "required": ["identifier", "decision_id", "chosen_option", "rationale", "resolved_by"]
            }),
        ),
        tool(
            "storypole_supersede_decision",
            "Supersede a previously resolved decision and return task to Blocked",
            json!({
                "type": "object",
                "properties": {
                    "identifier": { "type": "string" },
                    "decision_id": { "type": "string" },
                    "reason": { "type": "string" }
                },
                "required": ["identifier", "decision_id", "reason"]
            }),
        ),
        tool(
            "storypole_claim",
            "Acquire an exclusive lease claim on a task and set status to InProgress",
            json!({
                "type": "object",
                "properties": {
                    "identifier": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "branch": { "type": "string" }
                },
                "required": ["identifier", "agent_id"]
            }),
        ),
        tool(
            "storypole_heartbeat",
            "Renew lease TTL for an active InProgress claim",
            json!({
                "type": "object",
                "properties": {
                    "identifier": { "type": "string" },
                    "agent_id": { "type": "string" }
                },
                "required": ["identifier", "agent_id"]
            }),
        ),
        tool(
            "storypole_handoff",
            "Record structured handoff packet and release active lease",
            json!({
                "type": "object",
                "properties": {
                    "identifier": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "completed_items": { "type": "array", "items": { "type": "string" } },
                    "remaining_items": { "type": "array", "items": { "type": "string" } },
                    "last_error_or_blocker": { "type": "string" },
                    "git_ref": { "type": "string" },
                    "dirty_files": { "type": "array", "items": { "type": "string" } },
                    "target_status": { "type": "string", "enum": ["ready", "blocked", "failed"] }
                },
                "required": ["identifier", "agent_id"]
            }),
        ),
        tool(
            "storypole_verify",
            "Run verification gate or accept external receipt, marking task as Verified",
            json!({
                "type": "object",
                "properties": {
                    "identifier": { "type": "string" },
                    "run_command": { "type": "boolean" },
                    "verifier": { "type": "string" },
                    "summary": { "type": "string" }
                },
                "required": ["identifier"]
            }),
        ),
    ];
    tools.extend(crate::schema_plan::plan_tools());
    tools
}

pub(crate) fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
    })
}
