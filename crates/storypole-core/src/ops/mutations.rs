//! Basic task mutation operations: add, update, and slugification.

use crate::dag::validate_no_cycle;
use crate::error::StorypoleError;
use crate::index::query::{get_all_tasks, get_max_ticket_number, get_task_by_id};
use crate::model::LedgerEvent;
use crate::model::task::{TaskEdge, TaskKind, TaskNode, TaskScope, TaskStatus};
use crate::store::StorypoleStore;
use uuid::Uuid;

/// Parameters for creating a new task.
#[derive(Debug, Clone, Default)]
pub struct CreateTaskParams {
    pub title: String,
    pub description: String,
    pub kind: TaskKind,
    pub priority: u8,
    pub scope: TaskScope,
    pub acceptance_criteria: Vec<String>,
    pub status: Option<TaskStatus>,
    pub edges: Vec<TaskEdge>,
}

/// Parameters for updating an existing task.
#[derive(Debug, Clone, Default)]
pub struct UpdateTaskParams {
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<u8>,
    pub scope: Option<TaskScope>,
    pub acceptance_criteria: Option<Vec<String>>,
}

/// Convert a title string into a deterministic URL/CLI slug.
pub fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;
    for c in title.chars() {
        if c.is_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !slug.is_empty() {
            slug.push('-');
            prev_dash = true;
        }
    }
    slug.trim_end_matches('-').to_string()
}

/// Create and persist a new task node.
pub fn add_task(
    store: &mut StorypoleStore,
    params: CreateTaskParams,
) -> Result<TaskNode, StorypoleError> {
    store.config().validate_scope(
        params.scope.repo.as_deref(),
        params.scope.crate_name.as_deref(),
    )?;

    let prefix = store.config().ticket_prefix.clone();
    let max_num = get_max_ticket_number(store.conn(), &prefix)?;
    let ticket_key = format!("{}-{}", prefix, max_num + 1);
    let now = StorypoleStore::now_ms();
    let id = Uuid::new_v4();
    let slug = format!("{}-{}", slugify(&params.title), &id.to_string()[..6]);

    let initial_status = params.status.unwrap_or(if params.edges.is_empty() {
        TaskStatus::Ready
    } else {
        TaskStatus::Blocked
    });

    let task = TaskNode {
        id,
        ticket_key,
        slug,
        title: params.title,
        description: params.description,
        kind: params.kind,
        status: initial_status,
        priority: params.priority.clamp(1, 5),
        scope: params.scope,
        edges: params.edges,
        decisions: Vec::new(),
        acceptance_criteria: params.acceptance_criteria,
        active_claim: None,
        latest_handoff: None,
        verification_receipt: None,
        plan: None,
        created_at_epoch_ms: now,
        updated_at_epoch_ms: now,
    };

    let all_tasks = get_all_tasks(store.conn())?;
    for edge in &task.edges {
        validate_no_cycle(&all_tasks, task.id, edge.target_id, edge.kind)?;
    }

    store.record_event(&LedgerEvent::TaskCreated { task: task.clone() })?;
    Ok(task)
}

/// Update fields on an existing task node.
pub fn update_task(
    store: &mut StorypoleStore,
    task_id: Uuid,
    params: UpdateTaskParams,
) -> Result<TaskNode, StorypoleError> {
    let _existing = get_task_by_id(store.conn(), task_id)?;
    if let Some(ref sc) = params.scope {
        store
            .config()
            .validate_scope(sc.repo.as_deref(), sc.crate_name.as_deref())?;
    }

    let now = StorypoleStore::now_ms();
    let event = LedgerEvent::TaskUpdated {
        task_id,
        title: params.title,
        description: params.description,
        priority: params.priority,
        scope: params.scope,
        acceptance_criteria: params.acceptance_criteria,
        timestamp_ms: now,
    };

    store.record_event(&event)?;
    get_task_by_id(store.conn(), task_id)
}
