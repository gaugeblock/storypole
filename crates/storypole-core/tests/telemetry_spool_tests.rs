//! Tests for the verify-time trace spool (GB-165).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use gaugeblock_contracts::{AnonymizedSynthesisTrace, Language, TelemetryTier, VerificationReport};
use storypole_core::telemetry::{already_spooled, append_trace, spool_path};
use uuid::Uuid;

fn trace(task_key: Option<&str>, head: Option<&str>) -> AnonymizedSynthesisTrace {
    AnonymizedSynthesisTrace {
        trace_id: Uuid::new_v4(),
        language: Language::Rust,
        tier: TelemetryTier::Silver,
        selected_pattern_id: None,
        anonymized_scope_signature: "fn tokenize(input: &str) -> Vec<Token>".to_string(),
        filled_holes: vec![("body".to_string(), "input.split_whitespace()".to_string())],
        rejected_attempt: None,
        verification: VerificationReport {
            ast_valid: true,
            typecheck_passed: true,
            tests_passed: Some(true),
            diagnostics: vec![],
            duration_ms: 12,
        },
        task_key: task_key.map(str::to_string),
        head_commit_sha: head.map(str::to_string),
    }
}

#[test]
fn spool_path_sits_beside_the_storypole_directory() {
    let dot = std::path::Path::new("/ws/.gaugeblock/storypole");
    assert_eq!(
        spool_path(dot),
        std::path::Path::new("/ws/.gaugeblock/spool/outbox.jsonl"),
        "one outbox per canonical .gaugeblock"
    );
}

#[test]
fn a_trace_is_written_once_and_the_same_head_is_idempotent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = spool_path(&tmp.path().join(".gaugeblock").join("storypole"));

    let first = trace(Some("GB-165"), Some("abc123"));
    assert!(
        append_trace(&path, &first).expect("write"),
        "first write lands"
    );
    assert!(
        !append_trace(&path, &trace(Some("GB-165"), Some("abc123"))).expect("write"),
        "re-verifying the same head must not duplicate"
    );

    let written = std::fs::read_to_string(&path).expect("read");
    assert_eq!(written.lines().count(), 1, "exactly one record");

    // A new head for the same task is new work, so it is a second record.
    assert!(append_trace(&path, &trace(Some("GB-165"), Some("def456"))).expect("write"));
    assert_eq!(
        std::fs::read_to_string(&path)
            .expect("read")
            .lines()
            .count(),
        2
    );
}

#[test]
fn provenance_free_traces_are_never_treated_as_duplicates() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = spool_path(&tmp.path().join(".gaugeblock").join("storypole"));

    assert!(append_trace(&path, &trace(None, None)).expect("write"));
    assert!(append_trace(&path, &trace(None, None)).expect("write"));
    assert_eq!(
        std::fs::read_to_string(&path)
            .expect("read")
            .lines()
            .count(),
        2,
        "without a key there is nothing to deduplicate against"
    );
}

#[test]
fn lookups_survive_corrupt_lines() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = spool_path(&tmp.path().join(".gaugeblock").join("storypole"));
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
    std::fs::write(&path, "{not json}\n").expect("seed");

    assert!(
        !already_spooled(&path, Some("GB-1"), Some("sha")).expect("lookup"),
        "a corrupt line is skipped, not fatal"
    );
    assert!(append_trace(&path, &trace(Some("GB-1"), Some("sha"))).expect("write"));
}
