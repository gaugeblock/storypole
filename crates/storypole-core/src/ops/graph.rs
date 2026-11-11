//! Graph linking, deferral, and task splitting operations.

use crate::dag::validate_no_cycle;
use crate::error::StorypoleError;
use crate::index::query::{get_all_tasks, get_max_ticket_number, get_task_by_id};
use crate::model::LedgerEvent;
use crate::model::task::{EdgeKind, TaskEdge, TaskKind, TaskNode, TaskScope, TaskStatus};
use crate::ops::mutations::slugify;
use crate::store::StorypoleStore;
use uuid::Uuid;

/// The target of an edge, wherever it lives (`SP-48`).
///
/// Identity is the uuid; the key and repository are the locator a member-only checkout renders. Kept as a
/// small value rather than a `&TaskNode` because a target in another repository is read from *that*
/// repository's store, and the writer must not need the node - or a copy of it - to record the edge.
#[derive(Debug, Clone)]
pub struct EdgeTarget {
    /// Stable identity, which is what the edge stores.
    pub id: Uuid,
    /// Ticket key for rendering when the owning repository is not reachable.
    pub key: String,
    /// Owning repository, from the target's scope or from the store that holds it.
    pub repo: Option<String>,
    /// Whether the target is already verified, which decides whether `BlockedBy` blocks.
    pub verified: bool,
}

/// Link two tasks with a directed edge, verifying acyclicity.
pub fn link_tasks(
    store: &mut StorypoleStore,
    source_id: Uuid,
    target_id: Uuid,
    kind: EdgeKind,
) -> Result<(), StorypoleError> {
    let target = get_task_by_id(store.conn(), target_id)?;
    let target = EdgeTarget {
        id: target.id,
        key: target.ticket_key.clone(),
        repo: target.scope.repo.clone(),
        verified: target.status == TaskStatus::Verified,
    };
    link_to_target(store, source_id, &target, kind)
}

/// Link a task in this store to a target that may live in another repository (`SP-48`).
///
/// The edge is written into the *source's* store and carries the target's uuid plus its `repo:key`
/// locator. Never a copy of the target: two live copies of one task is how `GB-79` diverged, and a copy
/// would also go stale the moment the original is verified.
///
/// Cycle validation is per-store. The reachability walk only knows this store's tasks, so a cycle that
/// spans two repositories would not be seen here - detecting that needs the workspace rollup, and it is
/// stated rather than implied to work.
///
/// # Errors
///
/// [`StorypoleError::TaskNotFound`] when the source is not in this store - an edge belongs to its source,
/// so a foreign source has to be linked from the repository that owns it.
pub fn link_to_target(
    store: &mut StorypoleStore,
    source_id: Uuid,
    target: &EdgeTarget,
    kind: EdgeKind,
) -> Result<(), StorypoleError> {
    let source = get_task_by_id(store.conn(), source_id)?;

    let all_tasks = get_all_tasks(store.conn())?;
    validate_no_cycle(&all_tasks, source_id, target.id, kind)?;

    let now = StorypoleStore::now_ms();
    let edge = TaskEdge::new(kind, target.id).with_target_meta(&target.key, target.repo.as_deref());
    store.record_event(&LedgerEvent::EdgeAdded {
        task_id: source_id,
        edge,
        timestamp_ms: now,
    })?;

    if kind == EdgeKind::BlockedBy && !target.verified {
        store.record_event(&LedgerEvent::StatusChanged {
            task_id: source_id,
            from: source.status,
            to: TaskStatus::Blocked,
            reason: Some(format!("Blocked by unverified task {}", target.key)),
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
    let edge = TaskEdge::new(kind, target_id);
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
    // Two concurrent defers otherwise read the same maximum and both claim the next key (GB-185).
    let _alloc = super::ticket_lock::TicketLock::acquire(store.dot_dir())?;
    let prefix = store.config().ticket_prefix.clone();
    let max_num = get_max_ticket_number(store.conn(), &prefix)?;
    let ticket_key = format!("{}-{}", prefix, max_num + 1);
    let now = StorypoleStore::now_ms();
    let id = Uuid::new_v4();
    let slug = format!("{}-{}", slugify(&title), &id.to_string()[..6]);

    let mut edges = Vec::new();
    if let Some(src) = from_task {
        edges.push(TaskEdge::new(EdgeKind::DiscoveredFrom, src));
    }

    let status = match kind {
        TaskKind::Bug | TaskKind::TechDebt | TaskKind::Chore => TaskStatus::Triage,
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
        archived_at_epoch_ms: None,
        priority: 3,
        scope,
        edges,
        decisions: Vec::new(),
        acceptance_criteria: Vec::new(),
        active_claim: None,
        latest_handoff: None,
        verification_receipt: None,
        plan: None,
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
    let _alloc = super::ticket_lock::TicketLock::acquire(store.dot_dir())?;
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

        let mut edges = vec![
            TaskEdge::new(EdgeKind::ParentEpic, parent.id)
                .with_target_meta(&parent.ticket_key, parent.scope.repo.as_deref()),
        ];

        if sequential && let Some(prev) = prev_child_id {
            edges.push(TaskEdge::new(EdgeKind::BlockedBy, prev));
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
            archived_at_epoch_ms: None,
            priority: parent.priority,
            scope: parent.scope.clone(),
            edges,
            decisions: Vec::new(),
            acceptance_criteria: Vec::new(),
            active_claim: None,
            latest_handoff: None,
            verification_receipt: None,
            plan: None,
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
