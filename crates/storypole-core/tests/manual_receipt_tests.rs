//! `verify` refuses a manual receipt when a chain exists (`GB-334`).
//!
//! `GB-57` was verified with 0 of 6 plan steps and a manual receipt while the suite root had an executable chain
//! configured and never ran it. The receipt said `exit_code: Some(0)`, which is true of a command that was never
//! executed. A manual attestation is legitimate when nothing can run; it is a fiction when something can.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use gaugeblock_contracts::VerificationReceipt;
use gaugeblock_contracts::storypole::{TaskKind, TaskScope};
use storypole_core::config::StorypoleConfig;
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::ops::verification::verify_task;
use storypole_core::store::StorypoleStore;
use tempfile::TempDir;

fn config_with_a_chain() -> StorypoleConfig {
    StorypoleConfig {
        ticket_prefix: "SP".to_string(),
        verification_commands: vec![("default".to_string(), "true".to_string())],
        ..StorypoleConfig::default()
    }
}

fn receipt(verifier: &str) -> VerificationReceipt {
    VerificationReceipt {
        verifier: verifier.to_string(),
        command: "attested by hand".to_string(),
        exit_code: Some(0),
        diagnostic_count: None,
        summary: "trust me".to_string(),
        verified_at_epoch_ms: StorypoleStore::now_ms(),
        git_branch: None,
        base_commit_sha: None,
        head_commit_sha: None,
        commit_shas: Vec::new(),
        repo: None,
        chain_key: None,
    }
}

fn task(store: &mut StorypoleStore) -> uuid::Uuid {
    add_task(
        store,
        CreateTaskParams {
            title: "a ticket".to_string(),
            description: String::new(),
            kind: TaskKind::Feature,
            scope: TaskScope::default(),
            ..CreateTaskParams::default()
        },
    )
    .expect("add")
    .id
}

#[test]
fn a_manual_receipt_is_refused_while_a_chain_is_configured() {
    let temp = TempDir::new().unwrap();
    let mut store = StorypoleStore::init(temp.path(), config_with_a_chain()).expect("init");
    let id = task(&mut store);

    let error = verify_task(&mut store, id, Some(receipt("manual")), false, false)
        .expect_err("a chain exists and was not run");

    let message = error.to_string();
    assert!(
        message.contains("executable verification chain"),
        "{message}"
    );
    assert!(
        message.contains("default"),
        "and names the chain: {message}"
    );
}

#[test]
fn an_explicit_verifier_is_accepted_so_a_chain_that_cannot_run_has_a_way_out() {
    // The escape hatch matters: a ticket verified on a machine without the toolchain must be able to say who
    // attested it rather than pretending a chain ran.
    let temp = TempDir::new().unwrap();
    let mut store = StorypoleStore::init(temp.path(), config_with_a_chain()).expect("init");
    let id = task(&mut store);

    let verified = verify_task(
        &mut store,
        id,
        Some(receipt("release-manager")),
        false,
        false,
    )
    .expect("an explicit verifier is a deliberate attestation");

    assert_eq!(verified.verifier, "release-manager");
}

#[test]
fn a_manual_receipt_is_fine_when_no_chain_is_configured() {
    let temp = TempDir::new().unwrap();
    let mut store = StorypoleStore::init(temp.path(), StorypoleConfig::default()).expect("init");
    let id = task(&mut store);

    verify_task(&mut store, id, Some(receipt("manual")), false, false)
        .expect("nothing could have run, so attesting is the only option");
}
