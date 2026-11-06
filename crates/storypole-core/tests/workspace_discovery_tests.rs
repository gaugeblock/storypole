//! Discovery of the member repositories in a workspace (`GB-189`).
//!
//! The scan comes from `gaugeblock-workspace`; the qualification rule and the storypole-specific
//! enrichment (`dot_dir`, ticket prefix) stay here. That split is deliberate: the shared crate owns
//! "what members exist under a root", while "which directories count as a member" is a product
//! policy that differs per tool - storypole's rule accepts a plain directory holding `.storypole` or
//! `Cargo.toml`, not only a git checkout.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use storypole_core::discover_members;
use tempfile::TempDir;

fn touch(path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, "").unwrap();
}

#[test]
fn discovers_a_plain_directory_holding_storypole_state() {
    // The standalone guarantee: a workspace of ordinary directories that were never git repositories
    // must still work. Narrowing discovery to `.git` would silently return zero members here.
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join("alpha").join(".storypole")).unwrap();

    let members = discover_members(temp.path(), None);

    assert_eq!(members.len(), 1);
    assert_eq!(members[0].name, "alpha");
    assert!(
        members[0].dot_dir.is_some(),
        "the .storypole directory must be reported"
    );
}

#[test]
fn discovers_a_directory_by_its_cargo_manifest_or_git_dir() {
    let temp = TempDir::new().unwrap();
    touch(&temp.path().join("with-cargo").join("Cargo.toml"));
    fs::create_dir_all(temp.path().join("with-git").join(".git")).unwrap();

    let names: Vec<String> = discover_members(temp.path(), None)
        .into_iter()
        .map(|member| member.name)
        .collect();

    assert_eq!(
        names,
        vec!["with-cargo".to_string(), "with-git".to_string()]
    );
}

#[test]
fn skips_hidden_build_and_docs_directories() {
    let temp = TempDir::new().unwrap();
    touch(&temp.path().join(".hidden").join("Cargo.toml"));
    touch(&temp.path().join("target").join("Cargo.toml"));
    touch(&temp.path().join("docs").join("Cargo.toml"));
    touch(&temp.path().join("scripts").join("Cargo.toml"));
    touch(&temp.path().join("real").join("Cargo.toml"));

    let names: Vec<String> = discover_members(temp.path(), None)
        .into_iter()
        .map(|member| member.name)
        .collect();

    assert_eq!(names, vec!["real".to_string()]);
}

#[test]
fn members_are_sorted_by_name() {
    let temp = TempDir::new().unwrap();
    for name in ["zeta", "alpha", "miter"] {
        touch(&temp.path().join(name).join("Cargo.toml"));
    }

    let names: Vec<String> = discover_members(temp.path(), None)
        .into_iter()
        .map(|member| member.name)
        .collect();

    assert_eq!(
        names,
        vec!["alpha".to_string(), "miter".to_string(), "zeta".to_string()]
    );
}
