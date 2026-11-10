//! Tests for the verify-time trace spool (GB-165, GB-182).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use gaugeblock_contracts::{
    AnonymizedSynthesisTrace, Language, SpoolEntry, SyncStatus, TelemetryTier, VerificationReport,
};
use storypole_core::StorypoleConfig;
use storypole_core::store::StorypoleStore;
use storypole_core::telemetry::{
    already_spooled, append_trace, migrate_spool, normalize_spool_text, outbox_path, spool_path,
};
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

#[test]
fn legacy_bare_trace_lines_are_rewritten_into_envelopes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = spool_path(&tmp.path().join(".gaugeblock").join("storypole"));
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");

    let legacy = trace(Some("GB-165"), Some("9d5f83"));
    let joiner_trace = trace(None, None);
    let joiner_style = SpoolEntry {
        content_hash: SpoolEntry::hash_trace(&joiner_trace).expect("hash"),
        trace: joiner_trace,
        sync_status: SyncStatus::Synced,
        recorded_at_ms: 5,
    };
    let seeded = format!(
        "{}\n{}\n{{not json}}\n",
        serde_json::to_string(&legacy).expect("legacy line"),
        serde_json::to_string(&joiner_style).expect("envelope line")
    );
    std::fs::write(&path, seeded).expect("seed");

    assert_eq!(migrate_spool(&path).expect("migrate"), 1, "one bare line");

    let migrated = std::fs::read_to_string(&path).expect("read");
    let lines: Vec<&str> = migrated.lines().collect();
    assert_eq!(lines.len(), 3, "no line is dropped");

    let first: SpoolEntry = serde_json::from_str(lines[0]).expect("first is an envelope");
    assert_eq!(first.trace, legacy, "payload is preserved verbatim");
    assert_eq!(first.sync_status, SyncStatus::Pending);
    assert_eq!(
        first.content_hash,
        SpoolEntry::hash_trace(&legacy).expect("hash"),
        "the canonical hash is what rabbet recomputes"
    );

    let second: SpoolEntry = serde_json::from_str(lines[1]).expect("second is an envelope");
    assert_eq!(second, joiner_style, "an existing envelope is untouched");
    assert_eq!(lines[2], "{not json}", "unparsable lines survive");

    // Idempotent: nothing left to rewrite, and the file is not rewritten.
    assert_eq!(migrate_spool(&path).expect("migrate again"), 0);
    assert_eq!(std::fs::read_to_string(&path).expect("read"), migrated);
}

#[test]
fn migrating_a_missing_spool_is_a_no_op() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = spool_path(&tmp.path().join(".gaugeblock").join("storypole"));
    assert_eq!(migrate_spool(&path).expect("migrate"), 0);
    assert!(!path.exists(), "migration never creates an outbox");
}

#[test]
fn normalize_keeps_the_legacy_payload_readable_after_rewrite() {
    let legacy = trace(Some("GB-182"), Some("abc123"));
    let text = format!("{}\n", serde_json::to_string(&legacy).expect("line"));

    let (out, migrated) = normalize_spool_text(&text).expect("normalize");
    assert_eq!(migrated, 1);

    let entry: SpoolEntry = serde_json::from_str(out.trim_end()).expect("envelope");
    assert_eq!(entry.trace, legacy);
    assert_eq!(
        entry.trace.task_key.as_deref(),
        Some("GB-182"),
        "idempotency key survives the rewrite"
    );
    assert_eq!(entry.trace.head_commit_sha.as_deref(), Some("abc123"));
}

// --- which outbox a store writes to (SP-24) ------------------------------------------------------

/// A store in the suite layout. `init` picks the legacy `.storypole` directory when
/// `.gaugeblock/storypole` is absent, so the directory is created first and the legacy one removed.
fn store_in_suite_layout(root: &Path) -> StorypoleStore {
    fs::create_dir_all(root.join(".gaugeblock").join("storypole")).expect("store dir");
    let store = StorypoleStore::init(root, StorypoleConfig::default()).expect("init store");
    fs::remove_dir_all(root.join(".storypole")).ok();
    store
}

/// A suite that declares `members`, so membership resolves from a member checkout.
fn suite_declaring(root: &Path, members: &[&str]) -> StorypoleStore {
    let store = store_in_suite_layout(root);
    let list: Vec<String> = members.iter().map(|m| format!("\"{m}\"")).collect();
    fs::write(
        root.join(".gaugeblock").join("config.toml"),
        format!("[suite]\nmembers = [{}]\n", list.join(", ")),
    )
    .expect("suite declaration");
    store
}

/// The outbox below a directory. Callers pass a canonical root, because membership answers with the
/// resolved path and macOS reaches a tempdir through `/var` rather than `/private/var`.
fn expected_outbox(root: &Path) -> std::path::PathBuf {
    root.join(".gaugeblock").join("spool").join("outbox.jsonl")
}

/// A member checkout. `.git` is what `WorkspaceTopology::discover` qualifies a member by, so without
/// it the suite declares a member that cannot be resolved and membership answers `None`.
fn member_checkout(root: &Path) -> StorypoleStore {
    fs::create_dir_all(root.join(".git")).expect("member checkout");
    store_in_suite_layout(root)
}

#[test]
fn a_member_store_spools_into_the_suite_outbox() {
    // The defect SP-24 records: `verify --dir <member>` created `<member>/spool/outbox.jsonl` in four
    // repositories, so the trace existed and no consumer of the canonical spool could see it.
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = fs::canonicalize(tmp.path()).expect("canonical tempdir");
    let suite = suite_declaring(&root, &["miter"]);
    let member = member_checkout(&root.join("miter"));

    assert_eq!(
        outbox_path(&member),
        expected_outbox(&root),
        "a member writes the suite's outbox, not its own"
    );
    assert_eq!(
        outbox_path(&suite),
        outbox_path(&member),
        "the suite and its member must name one file"
    );
}

#[test]
fn a_lone_checkout_keeps_its_own_outbox() {
    // The other half of the rule: a repository no suite lists is its own corpus, and centralising must
    // not reach outside a suite to find a spool it has no business writing.
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = fs::canonicalize(tmp.path()).expect("canonical tempdir");
    let lone = store_in_suite_layout(&root);

    assert_eq!(outbox_path(&lone), expected_outbox(&root));
}
