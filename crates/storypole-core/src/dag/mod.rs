//! Topological sorting, cycle detection, and DAG validation.

pub mod briefing;

use crate::error::StorypoleError;
use crate::model::decision::DecisionStatus;
use crate::model::task::{EdgeKind, TaskNode, TaskStatus};
use std::collections::{BTreeMap, HashSet, VecDeque};
use uuid::Uuid;

/// Check whether adding a directed edge would create a cycle in the DAG.
/// Only `BlockedBy` and `ParentEpic` edges are subject to strict acyclic rules.
pub fn validate_no_cycle(
    tasks: &BTreeMap<Uuid, TaskNode>,
    source_id: Uuid,
    target_id: Uuid,
    kind: EdgeKind,
) -> Result<(), StorypoleError> {
    if !matches!(kind, EdgeKind::BlockedBy | EdgeKind::ParentEpic) {
        return Ok(());
    }

    if source_id == target_id {
        return Err(StorypoleError::CycleDetected(format!(
            "Self-referential edge on task {source_id}"
        )));
    }

    // Path check: if source_id is reachable from target_id following the same kind of edge,
    // then adding source -> target creates a cycle.
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(target_id);

    while let Some(current) = queue.pop_front() {
        if current == source_id {
            return Err(StorypoleError::CycleDetected(format!(
                "Adding {:?} edge from {} to {} introduces a directed cycle",
                kind, source_id, target_id
            )));
        }

        if !visited.insert(current) {
            continue;
        }

        if let Some(node) = tasks.get(&current) {
            for edge in &node.edges {
                if edge.kind == kind && !visited.contains(&edge.target_id) {
                    queue.push_back(edge.target_id);
                }
            }
        }
    }

    Ok(())
}

/// Compute a topological ordering of tasks based on `BlockedBy` dependencies.
pub fn topological_sort(tasks: &BTreeMap<Uuid, TaskNode>) -> Result<Vec<Uuid>, StorypoleError> {
    let mut in_degree: BTreeMap<Uuid, usize> = BTreeMap::new();
    let mut adj: BTreeMap<Uuid, Vec<Uuid>> = BTreeMap::new();

    for &id in tasks.keys() {
        in_degree.insert(id, 0);
        adj.insert(id, Vec::new());
    }

    for (&id, task) in tasks {
        for edge in &task.edges {
            if edge.kind == EdgeKind::BlockedBy && tasks.contains_key(&edge.target_id) {
                // edge.target_id must be completed before id (target -> id)
                if let Some(list) = adj.get_mut(&edge.target_id) {
                    list.push(id);
                }
                *in_degree.entry(id).or_insert(0) += 1;
            }
        }
    }

    let mut queue: VecDeque<Uuid> = in_degree
        .iter()
        .filter_map(|(&id, &deg)| if deg == 0 { Some(id) } else { None })
        .collect();

    let mut order = Vec::new();
    while let Some(u) = queue.pop_front() {
        order.push(u);
        if let Some(neighbors) = adj.get(&u) {
            for &v in neighbors {
                if let Some(deg) = in_degree.get_mut(&v) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(v);
                    }
                }
            }
        }
    }

    if order.len() != tasks.len() {
        return Err(StorypoleError::CycleDetected(
            "Topological cycle detected in task dependency graph".to_string(),
        ));
    }

    Ok(order)
}

/// Determine if all hard dependencies and decisions for a task are satisfied.
pub fn is_task_dependency_ready(task: &TaskNode, all_tasks: &BTreeMap<Uuid, TaskNode>) -> bool {
    // 1. Any pending decision blocks readiness
    for decision in &task.decisions {
        if decision.status == DecisionStatus::Pending {
            return false;
        }
    }

    // 2. All BlockedBy dependencies must be Verified
    for edge in &task.edges {
        if edge.kind == EdgeKind::BlockedBy {
            match all_tasks.get(&edge.target_id) {
                Some(parent)
                    if parent.status == TaskStatus::Verified
                        || parent.status == TaskStatus::Archived =>
                {
                    continue;
                }
                _ => return false,
            }
        }
    }

    true
}
