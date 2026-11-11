//! The workspace fixture several receipt tests share (`SP-50`).
//!
//! Extracted when `receipt_provenance_tests.rs` crossed the 250-line limit: the file that grew was the
//! test file, so the fixture moved rather than the assertions being trimmed. A `support/` subdirectory
//! is compiled into each test crate that declares it and is not itself a test target.

use std::fs;
use std::path::Path;

use storypole_core::config::StorypoleConfig;
use storypole_core::store::StorypoleStore;

/// A workspace store at `root` with a `miter` member and two verification chains.
pub fn fixture(root: &Path) -> StorypoleStore {
    fs::create_dir_all(root.join("miter").join("src")).expect("member dir");
    fs::write(root.join("miter").join("Cargo.toml"), "[workspace]\n").expect("manifest");
    // `.git` is what `WorkspaceTopology::discover` qualifies a member by, and the suite root the store
    // sits in is the fixture root, so the crate-to-repository map can see the member (`SP-50`).
    fs::create_dir_all(root.join("miter").join(".git")).expect("member checkout");
    // One crate the member declares, so a task scoped only by crate name has evidence of its owner.
    fs::create_dir_all(root.join("miter").join("crates").join("miter-core")).expect("member crate");
    fs::write(
        root.join("miter")
            .join("crates")
            .join("miter-core")
            .join("Cargo.toml"),
        "[package]\nname = \"miter-core\"\nversion = \"0.1.0\"\n",
    )
    .expect("crate manifest");
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
