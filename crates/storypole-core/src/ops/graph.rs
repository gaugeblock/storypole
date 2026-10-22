//! Graph linking, deferral, and task splitting operations.

use crate::dag::validate_no_cycle;
use crate::error::StorypoleError;
use crate::index::query::{get_all_tasks, get_max_ticket_number, get_task_by_id};
use crate::model::LedgerEvent;
use crate::model::task::{EdgeKind, TaskEdge, TaskKind, TaskNode, TaskScope, TaskStatus};
use crate::ops::mutations::slugify;
use crate::store::StorypoleStore;
use uuid::Uuid;

/// Link two tasks with a directed edge, verifying acyclicity.
pub fn link_tasks(
    store: &mut StorypoleStore,
    source_id: Uuid,
    target_id: Uuid,
    kind: EdgeKind,
) -> Result<(), StorypoleError> {
    let source = get_task_by_id(store.conn(), source_id)?;
    let target = get_task_by_id(store.conn(), target_id)?;

    let all_tasks = get_all_tasks(store.conn())?;
    validate_no_cycle(&all_tasks, source_id, target_id, kind)?;

    let now = StorypoleStore::now_ms();
    let edge = TaskEdge { kind, target_id };
    store.record_event(&LedgerEvent::EdgeAdded {
        task_id: source_id,
        edge,
        timestamp_ms: now,
    })?;

    if kind == EdgeKind::BlockedBy && target.status != TaskStatus::Verified {
        store.record_event(&LedgerEvent::StatusChanged {
            task_id: source_id,
            from: source.status,
            to: TaskStatus::Blocked,
            reason: Some(format!("Blocked by unverified task {}", target.ticket_key)),
            timestamp_ms: now,
        })?;
    }

    Ok(())
}

/// Remove a directed edge between two tasks.
pub fn unlink_tasks(
    store: &mut StorypoleStore,
    source_id: Uuid,
    target_id: Uuid,
    kind: EdgeKind,
) -> Result<(), StorypoleError> {
    let now = StorypoleStore::now_ms();
    let edge = TaskEdge { kind, target_id };
    store.record_event(&LedgerEvent::EdgeRemoved {
        task_id: source_id,
        edge,
        timestamp_ms: now,
    })?;
    Ok(())
}

/// Defer an out-of-band issue or tech-debt cleanly in <5ms.
pub fn defer_issue(
    store: &mut StorypoleStore,
    title: String,
    description: String,
    kind: TaskKind,
    scope: TaskScope,
    from_task: Option<Uuid>,
    captured_by: String,
) -> Result<TaskNode, StorypoleError> {
    let prefix = store.config().ticket_prefix.clone();
    let max_num = get_max_ticket_number(store.conn(), &prefix)?;
    let ticket_key = format!("{}-{}", prefix, max_num + 1);
    let now = StorypoleStore::now_ms();
    let id = Uuid::new_v4();
    let slug = format!("{}-{}", slugify(&title), &id.to_string()[..6]);

    let mut edges = Vec::new();
    if let Some(src) = from_task {
        edges.push(TaskEdge {
            kind: EdgeKind::DiscoveredFrom,
            target_id: src,
        });
    }

    let status = match kind {
        TaskKind::Bug | TaskKind::TechDebt => TaskStatus::Triage,
        _ => TaskStatus::Deferred,
    };

    let task = TaskNode {
        id,
        ticket_key,
        slug,
        title,
        description,
        kind,
        status,
        priority: 3,
        scope,
        edges,
        decisions: Vec::new(),
        acceptance_criteria: Vec::new(),
        active_claim: None,
        latest_handoff: None,
        verification_receipt: None,
        created_at_epoch_ms: now,
        updated_at_epoch_ms: now,
    };

    store.record_event(&LedgerEvent::DeferredItemCaptured {
        task: task.clone(),
        captured_by_agent: captured_by,
        discovered_from_task_id: from_task,
    })?;

    Ok(task)
}

/// Split an oversized task into a parent Epic and multiple child tasks.
pub fn split_task(
    store: &mut StorypoleStore,
    parent_id: Uuid,
    child_titles: Vec<String>,
    sequential: bool,
) -> Result<Vec<TaskNode>, StorypoleError> {
    let parent = get_task_by_id(store.conn(), parent_id)?;
    let prefix = store.config().ticket_prefix.clone();
    let mut current_max = get_max_ticket_number(store.conn(), &prefix)?;
    let now = StorypoleStore::now_ms();

    let mut children = Vec::new();
    let mut prev_child_id: Option<Uuid> = None;

    for (idx, title) in child_titles.into_iter().enumerate() {
        current_max += 1;
        let child_id = Uuid::new_v4();
        let ticket_key = format!("{prefix}-{current_max}");
        let slug = format!("{}-{}", slugify(&title), &child_id.to_string()[..6]);

        let mut edges = vec![TaskEdge {
            kind: EdgeKind::ParentEpic,
            target_id: parent.id,
        }];

        if sequential && let Some(prev) = prev_child_id {
            edges.push(TaskEdge {
                kind: EdgeKind::BlockedBy,
                target_id: prev,
            });
        }

        let child_status = if idx == 0 || !sequential {
            TaskStatus::Ready
        } else {
            TaskStatus::Blocked
        };

        let child = TaskNode {
            id: child_id,
            ticket_key,
            slug,
            title,
            description: format!("Subtask of Epic {}", parent.ticket_key),
            kind: TaskKind::Feature,
            status: child_status,
            priority: parent.priority,
            scope: parent.scope.clone(),
            edges,
            decisions: Vec::new(),
            acceptance_criteria: Vec::new(),
            active_claim: None,
            latest_handoff: None,
            verification_receipt: None,
            created_at_epoch_ms: now,
            updated_at_epoch_ms: now,
        };

        prev_child_id = Some(child_id);
        children.push(child);
    }

    store.record_event(&LedgerEvent::TaskSplit {
        parent_task_id: parent_id,
        spawned_children: children.clone(),
        timestamp_ms: now,
    })?;

    Ok(children)
}
