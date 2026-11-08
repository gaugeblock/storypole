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
use storypole_core::ops::verification::verify_task;
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
