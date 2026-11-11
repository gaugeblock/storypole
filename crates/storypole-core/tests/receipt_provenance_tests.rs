//! A receipt must say which repository and which chain it measured (`GB-316`).
//!
//! The defect these tests pin: `GB-272`, `GB-273`, `GB-275` and `GB-276` were member-repository work,
//! and their receipts named the harness branch and the harness commit count. Nothing in those receipts
//! was false; they simply did not say what they were about, so a harness chain and a member chain
//! produced indistinguishable evidence.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use storypole_core::config::StorypoleConfig;
use storypole_core::model::task::TaskScope;
use storypole_core::ops::mutations::{CreateTaskParams, add_task};
use storypole_core::ops::verification::{verify_task, verify_task_with_note};
use storypole_core::store::StorypoleStore;
use tempfile::tempdir;

/// A workspace store at `root` with a `miter` member and two verification chains.
fn fixture(root: &Path) -> StorypoleStore {
    fs::create_dir_all(root.join("miter").join("src")).expect("member dir");
    fs::write(root.join("miter").join("Cargo.toml"), "[workspace]\n").expect("manifest");
    fs::create_dir_all(root.join(".gaugeblock").join("storypole")).expect("store dir");

    // `true` exits 0 without producing output: the chain identity is what is under test, not the work.
    let config = StorypoleConfig {
        verification_commands: vec![
            ("miter".to_string(), "true".to_string()),
            ("default".to_string(), "true".to_string()),
        ],
        ..StorypoleConfig::default()
    };
    let store = StorypoleStore::init(root, config).expect("init");
    fs::remove_dir_all(root.join(".storypole")).ok();
    store
}

#[test]
fn a_member_receipt_names_its_repository_and_its_chain() {
    let tmp = tempdir().expect("tempdir");
    let mut store = fixture(tmp.path());

    let member = add_task(
        &mut store,
        CreateTaskParams {
            title: "Member work".into(),
            scope: TaskScope {
                repo: Some("miter".into()),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("add member task");

    let receipt =
        verify_task(&mut store, member.id, None, true, false).expect("verify member task");

    assert_eq!(
        receipt.repo.as_deref(),
        Some("miter"),
        "the receipt names the repository it measured"
    );
    assert_eq!(
        receipt.chain_key.as_deref(),
        Some("miter"),
        "the receipt names the chain that ran"
    );
}

#[test]
fn a_harness_receipt_does_not_claim_a_repository_or_a_member_chain() {
    let tmp = tempdir().expect("tempdir");
    let mut store = fixture(tmp.path());

    let harness = add_task(
        &mut store,
        CreateTaskParams {
            title: "Harness work".into(),
            ..Default::default()
        },
    )
    .expect("add harness task");

    let receipt =
        verify_task(&mut store, harness.id, None, true, false).expect("verify harness task");

    assert_eq!(
        receipt.repo, None,
        "harness work names no repository, rather than borrowing one"
    );
    assert_eq!(
        receipt.chain_key.as_deref(),
        Some("default"),
        "and the fallback chain is named as the fallback"
    );
}

#[test]
fn a_repository_chain_is_reached_even_when_the_crate_has_none() {
    // The defect this pins: the lookup fell back to `default` on the crate key, so a task naming both a
    // crate and a repository never reached the repository's chain and the harness chain ran instead.
    let tmp = tempdir().expect("tempdir");
    let mut store = fixture(tmp.path());

    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Crate without a chain".into(),
            scope: TaskScope {
                crate_name: Some("miter-core".into()),
                repo: Some("miter".into()),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("add task");

    let receipt = verify_task(&mut store, task.id, None, true, false).expect("verify");

    assert_eq!(
        receipt.chain_key.as_deref(),
        Some("miter"),
        "the repository chain is reached after the crate lookup misses"
    );
}

#[test]
fn a_note_annotates_the_executed_receipt_instead_of_replacing_it() {
    // SP-49: `verify --summary` built a manual override - verifier manual, command n/a, exit_code 0, no
    // chain key - so the flag whose purpose is to record what was verified replaced the run with an
    // attestation that ran nothing. The note now rides along and the evidence stays the chain's.
    let tmp = tempdir().expect("tempdir");
    let mut store = fixture(tmp.path());

    let task = add_task(
        &mut store,
        CreateTaskParams {
            title: "Annotated work".into(),
            scope: TaskScope {
                repo: Some("miter".into()),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("add task");

    let receipt = verify_task_with_note(
        &mut store,
        task.id,
        None,
        true,
        false,
        Some("checked the spool route by hand"),
    )
    .expect("verify with a note");

    assert_eq!(
        receipt.verifier, "storypole-verify",
        "the chain attests, not a person who typed a summary"
    );
    assert_eq!(
        receipt.command, "true",
        "the receipt names the command that ran, not n/a"
    );
    assert_eq!(receipt.exit_code, Some(0));
    assert_eq!(
        receipt.chain_key.as_deref(),
        Some("miter"),
        "the chain key survives the note"
    );
    assert!(
        receipt
            .summary
            .starts_with("checked the spool route by hand"),
        "the note is attached ahead of the output, got: {}",
        receipt.summary
    );
}

#[test]
fn two_receipts_from_different_chains_do_not_read_the_same() {
    let tmp = tempdir().expect("tempdir");
    let mut store = fixture(tmp.path());

    let member = add_task(
        &mut store,
        CreateTaskParams {
            title: "Member work".into(),
            scope: TaskScope {
                repo: Some("miter".into()),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("add member task");
    let harness = add_task(
        &mut store,
        CreateTaskParams {
            title: "Harness work".into(),
            ..Default::default()
        },
    )
    .expect("add harness task");

    let member_receipt =
        verify_task(&mut store, member.id, None, true, false).expect("verify member task");
    let harness_receipt =
        verify_task(&mut store, harness.id, None, true, false).expect("verify harness task");

    assert_ne!(
        (
            member_receipt.repo.clone(),
            member_receipt.chain_key.clone()
        ),
        (
            harness_receipt.repo.clone(),
            harness_receipt.chain_key.clone()
        ),
        "different repositories and different chains must be distinguishable from the receipt alone"
    );
}
