//! Lease behaviour (`GB-333`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn the_first_agent_gets_the_lease_and_the_second_is_told_who_has_it() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    assert_eq!(
        acquire(dir, "SP-1", "agent-a", 2_000, 1_000).unwrap(),
        Acquire::Granted
    );
    assert_eq!(
        acquire(dir, "SP-1", "agent-b", 2_000, 1_000).unwrap(),
        Acquire::Held(Lease {
            ticket: "SP-1".to_string(),
            agent: "agent-a".to_string(),
            expires_at_ms: 2_000,
        })
    );
}

#[test]
fn the_same_agent_renewing_its_own_lease_is_granted() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    acquire(dir, "SP-2", "agent-a", 2_000, 1_000).unwrap();

    assert_eq!(
        acquire(dir, "SP-2", "agent-a", 5_000, 1_500).unwrap(),
        Acquire::Granted
    );
    assert_eq!(
        holder(dir, "SP-2", 1_500).unwrap().unwrap().expires_at_ms,
        5_000
    );
}

#[test]
fn an_expired_lease_is_taken_over_rather_than_blocking_the_ticket() {
    // A dead agent must not hold work hostage until somebody cleans up by hand.
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    acquire(dir, "SP-3", "agent-a", 2_000, 1_000).unwrap();

    assert_eq!(
        acquire(dir, "SP-3", "agent-b", 9_000, 3_000).unwrap(),
        Acquire::Granted
    );
    assert_eq!(
        holder(dir, "SP-3", 3_000).unwrap().unwrap().agent,
        "agent-b"
    );
}

#[test]
fn release_only_works_for_the_holder_and_clears_the_lease() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    acquire(dir, "SP-4", "agent-a", 2_000, 1_000).unwrap();

    assert!(!release(dir, "SP-4", "agent-b").unwrap(), "not the holder");
    assert!(release(dir, "SP-4", "agent-a").unwrap());
    assert_eq!(holder(dir, "SP-4", 1_000).unwrap(), None);
    assert!(
        matches!(
            acquire(dir, "SP-4", "agent-b", 9_000, 1_500).unwrap(),
            Acquire::Granted
        ),
        "and the ticket is free again"
    );
}

#[test]
fn no_lock_is_left_behind() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    acquire(dir, "SP-5", "agent-a", 2_000, 1_000).unwrap();

    let leftovers: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".lock") || name.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "nothing lingers: {leftovers:?}");
}
