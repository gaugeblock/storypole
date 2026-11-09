//! Deciding where each ticket belongs, before anything moves (`GB-331`).
//!
//! The rules were settled before this module existed: a ticket's owner is its `scope.repo`; a ticket with no
//! repository is adopted into the repository that owns its crate when the evidence is unambiguous; everything
//! else stays at the root as suite-level work. This module applies them and reports the evidence for every
//! decision, so the plan can be read rather than trusted.
//!
//! Nothing here writes. Splitting a store is destructive in the working tree, so the plan is inspectable first -
//! and the acceptance test is a differential replay: replaying the resulting records must produce the same task
//! states as replaying the ledger they came from.

use std::collections::BTreeMap;

use uuid::Uuid;

use crate::model::TaskNode;
use crate::state::LoadedState;

/// Repository names that mean "the suite itself" rather than a member.
const SUITE_SCOPES: [&str; 2] = ["gaugeblock", "workspace"];

/// Pseudo-crate names that describe the suite rather than any repository.
const SUITE_CRATES: [&str; 2] = ["workspace", "gaugeblock"];

/// Where a ticket belongs after the split.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// Stays in the root ledger: suite-level, or no evidence to move it.
    Root,
    /// Belongs to a member repository.
    Member(String),
}

/// One ticket's routing, with the reason for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    /// Human-facing key.
    pub ticket_key: String,
    /// Stable id, which is what the differential replay compares.
    pub task_id: Uuid,
    /// Where it goes.
    pub destination: Destination,
    /// Why, in one word: `scope`, `crate`, `suite`, `unclaimed` or `unscoped`.
    pub evidence: &'static str,
}

/// The routing of every ticket in a store.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    /// One entry per ticket, ordered by key number.
    pub assignments: Vec<Assignment>,
}

impl Plan {
    /// Tickets moving to a member repository, counted per repository.
    #[must_use]
    pub fn per_member(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for assignment in &self.assignments {
            if let Destination::Member(repo) = &assignment.destination {
                *counts.entry(repo.clone()).or_insert(0) += 1;
            }
        }
        counts
    }

    /// Tickets that stay at the root.
    #[must_use]
    pub fn staying(&self) -> usize {
        self.assignments
            .iter()
            .filter(|assignment| assignment.destination == Destination::Root)
            .count()
    }

    /// Tickets adopted because their crate named their owner.
    #[must_use]
    pub fn adopted(&self) -> usize {
        self.assignments
            .iter()
            .filter(|assignment| assignment.evidence == "crate")
            .count()
    }
}

/// Routes every task in `state`, using `crate_owner` to attribute an unscoped ticket by its crate.
///
/// `crate_owner` is a parameter rather than a lookup here because the crate-to-repository map belongs to the
/// workspace topology, which reads manifests; this module stays testable without inventing a fleet.
#[must_use]
pub fn plan(
    state: &LoadedState,
    members: &[String],
    crate_owner: &dyn Fn(&str) -> Option<String>,
) -> Plan {
    let mut assignments: Vec<Assignment> = state
        .tasks
        .values()
        .map(|task| assign(task, members, crate_owner))
        .collect();
    assignments.sort_by_key(|assignment| natural_key(&assignment.ticket_key));
    Plan { assignments }
}

fn assign(
    task: &TaskNode,
    members: &[String],
    crate_owner: &dyn Fn(&str) -> Option<String>,
) -> Assignment {
    let (destination, evidence) = match task.scope.repo.as_deref() {
        Some(repo) if members.iter().any(|member| member == repo) => {
            (Destination::Member(repo.to_string()), "scope")
        }
        Some(repo) if SUITE_SCOPES.contains(&repo) => (Destination::Root, "suite"),
        // A repository name no member claims stays put: routing to a repository that does not exist would
        // create a store nobody reads, and saying so is more useful than filing it as suite work.
        Some(_) => (Destination::Root, "unclaimed"),
        None => match task
            .scope
            .crate_name
            .as_deref()
            .filter(|name| !SUITE_CRATES.contains(name))
            .and_then(crate_owner)
            .filter(|repo| members.iter().any(|member| member == repo))
        {
            Some(repo) => (Destination::Member(repo), "crate"),
            None => (Destination::Root, "unscoped"),
        },
    };
    Assignment {
        ticket_key: task.ticket_key.clone(),
        task_id: task.id,
        destination,
        evidence,
    }
}

/// Sorts `GB-9` before `GB-10`, which a plain string sort does not.
fn natural_key(key: &str) -> (String, u64) {
    let (prefix, number) = key.rsplit_once('-').unwrap_or((key, "0"));
    (prefix.to_string(), number.parse::<u64>().unwrap_or(0))
}

#[cfg(test)]
mod tests;
