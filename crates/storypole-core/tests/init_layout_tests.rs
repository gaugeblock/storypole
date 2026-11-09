//! `init` writes the canonical layout, and an older one still opens (`GB-344`).
//!
//! `init` used to fall back to `.storypole` whenever `.gaugeblock` did not exist - which is exactly the
//! case in a repository being initialised - so a new repository got the pre-ADR-0001 layout and the
//! canonical one appeared only if something else created `.gaugeblock` first.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use storypole_core::config::StorypoleConfig;
use storypole_core::storage_mode::StorageMode;
use storypole_core::store::StorypoleStore;
use tempfile::TempDir;

#[test]
fn init_creates_the_canonical_directory_and_not_the_legacy_one() {
    let temp = TempDir::new().unwrap();

    StorypoleStore::init(temp.path(), StorypoleConfig::default()).expect("init");

    assert!(
        temp.path()
            .join(".gaugeblock/storypole/config.toml")
            .is_file(),
        "the store lives where ADR 0001 says"
    );
    assert!(
        !temp.path().join(".storypole").exists(),
        "and nothing is written to the legacy location"
    );
}

#[test]
fn a_store_initialised_by_init_reopens_on_the_same_directory() {
    // The test that would have caught the original defect: before the fix, init wrote `.storypole` and the
    // reopen resolved `.gaugeblock/storypole` if anything had created it, so the two disagreed.
    let temp = TempDir::new().unwrap();
    let created = StorypoleStore::init(temp.path(), StorypoleConfig::default()).expect("init");
    let reopened = StorypoleStore::open_exact(temp.path()).expect("reopen");

    assert_eq!(created.dot_dir(), reopened.dot_dir());
    assert_eq!(
        created.dot_dir(),
        temp.path().join(".gaugeblock").join("storypole")
    );
}

#[test]
fn an_existing_legacy_store_still_opens() {
    // Backwards compatibility is the reason `init` was cautious; the caution belonged in resolution, where
    // it already is.
    let temp = TempDir::new().unwrap();
    let legacy = temp.path().join(".storypole");
    fs::create_dir_all(&legacy).unwrap();
    StorypoleConfig::default()
        .save_to_file(&legacy.join("config.toml"))
        .unwrap();
    fs::File::create(legacy.join("ledger.jsonl")).unwrap();

    let store = StorypoleStore::open_exact(temp.path()).expect("legacy store opens");

    assert_eq!(store.dot_dir(), legacy);
    assert_eq!(store.storage_mode(), StorageMode::Ledger);
}

#[test]
fn a_repository_with_gaugeblock_but_no_store_gets_the_canonical_directory() {
    // The shape `common` has: `.gaugeblock` exists for docs.toml, and nothing else. The old fallback returned
    // `.storypole` here, so a migration would have created a legacy store in a repository that never had one.
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join(".gaugeblock")).unwrap();
    fs::write(temp.path().join(".gaugeblock").join("docs.toml"), "").unwrap();

    let dot = StorypoleStore::resolve_dot_dir(temp.path());

    assert_eq!(
        dot,
        temp.path().join(".gaugeblock").join("storypole"),
        "a new store belongs in the canonical location"
    );
}

#[test]
fn a_repository_with_only_a_legacy_directory_keeps_it() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join(".storypole")).unwrap();

    assert_eq!(
        StorypoleStore::resolve_dot_dir(temp.path()),
        temp.path().join(".storypole")
    );
}
