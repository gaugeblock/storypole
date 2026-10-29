//! Acceptance tests for verify-time trace capture (GB-165).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

use gaugeblock_contracts::{SpoolEntry, SyncStatus, TelemetryTier};
use storypole_core::config::StorypoleConfig;
use storypole_core::model::task::TaskScope;
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::ops::workflow::verify_task;
use storypole_core::store::StorypoleStore;

fn git(dir: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git");
    assert!(status.status.success(), "git {:?} failed", args);
}

fn repo_with_commit() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(tmp.path().join(".gaugeblock").join("storypole")).expect("layout");
    git(tmp.path(), &["init", "--initial-branch=main"]);
    git(tmp.path(), &["config", "user.email", "test@gaugeblock.dev"]);
    git(tmp.path(), &["config", "user.name", "Test"]);
    std::fs::write(tmp.path().join("README.md"), "hello\n").expect("write");
    git(tmp.path(), &["add", "README.md"]);
    git(tmp.path(), &["commit", "-m", "chore: init"]);
    tmp
}

#[test]
fn verifying_writes_one_silver_trace_and_re_verifying_writes_none() {
    let tmp = repo_with_commit();
    let mut store = StorypoleStore::init(tmp.path(), StorypoleConfig::default()).expect("init");

    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Trace me".into(),
            scope: TaskScope {
                repo: Some("gaugeblock".into()),
                target_symbols: vec!["tokenizer::scan".into()],
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("task");

    verify_task(&mut store, task.id, None, false, false).expect("verify");

    let spool = tmp
        .path()
        .join(".gaugeblock")
        .join("spool")
        .join("outbox.jsonl");
    let first = std::fs::read_to_string(&spool).expect("spool exists after verify");
    assert_eq!(first.lines().count(), 1, "one trace for one verified head");

    let record: SpoolEntry = serde_json::from_str(first.lines().next().unwrap()).expect("json");
    assert_eq!(
        record.trace.task_key.as_deref(),
        Some(task.ticket_key.as_str()),
        "provenance records the ticket"
    );
    assert_eq!(record.trace.tier, TelemetryTier::Silver);
    assert!(
        record.trace.head_commit_sha.is_some(),
        "provenance records the verified head: {record:?}"
    );
    assert_eq!(record.trace.filled_holes[0].0, "tokenizer::scan");
    assert_eq!(
        record.content_hash,
        SpoolEntry::hash_trace(&record.trace).expect("hash"),
        "the envelope carries the canonical hash rabbet recomputes"
    );
    assert_eq!(record.sync_status, SyncStatus::Pending);

    // Re-verifying the same head is a no-op: no duplicate trace.
    verify_task(&mut store, task.id, None, false, false).expect("verify again");
    assert_eq!(
        std::fs::read_to_string(&spool)
            .expect("read")
            .lines()
            .count(),
        1,
        "re-verifying the same head must not duplicate the trace"
    );
}
