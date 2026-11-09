//! Which repository owns which crate, read from manifests (`GB-331`).
//!
//! Adoption needs to answer "who owns `benchwork-core`?" with evidence rather than a guess, and the evidence is
//! the manifest: a crate is owned by the repository whose workspace declares it. Reading names only - not
//! versions, features or dependencies - keeps this small and keeps a failed parse from silently attributing work
//! to the wrong repository, because a crate it cannot read is a crate it does not claim.
//!
//! Two crates claiming one another's repository is not possible here: the caller supplies one member at a time.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Maps every crate name found under the given repositories to the repository that declares it.
///
/// `members` pairs a repository name with its checkout root. A member whose manifests cannot be read contributes
/// nothing rather than a guess, so an unreadable repository means "no evidence" and the ticket stays put.
#[must_use]
pub fn crate_owners(members: &[(String, std::path::PathBuf)]) -> BTreeMap<String, String> {
    let mut owners = BTreeMap::new();
    for (repo, root) in members {
        for manifest in manifests_of(root) {
            if let Some(name) = package_name(&manifest) {
                owners.entry(name).or_insert_with(|| repo.clone());
            }
        }
    }
    owners
}

/// The manifests that can declare a package: the root one and one level of `crates/*`.
fn manifests_of(root: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let root_manifest = root.join("Cargo.toml");
    if root_manifest.is_file() {
        found.push(root_manifest);
    }
    if let Ok(entries) = fs::read_dir(root.join("crates")) {
        let mut nested: Vec<std::path::PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path().join("Cargo.toml"))
            .filter(|path| path.is_file())
            .collect();
        nested.sort();
        found.extend(nested);
    }
    found
}

/// `[package] name = "..."`, ignoring a manifest that only declares a workspace.
///
/// The `[package]` section is found by name rather than by position, because a virtual manifest starts with
/// `[workspace]` and reading its first `name` would attribute the workspace itself to a crate.
fn package_name(manifest: &Path) -> Option<String> {
    let text = fs::read_to_string(manifest).ok()?;
    let mut in_package = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if in_package
            && let Some(rest) = line.strip_prefix("name")
            && let Some(value) = rest.trim_start().strip_prefix('=')
        {
            return Some(value.trim().trim_matches('"').to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn a_member_repository_owns_its_root_and_nested_crates() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("benchwork");
        write(
            &root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n\n[package]\nname = \"benchwork\"\n",
        );
        write(
            &root.join("crates/benchwork-core/Cargo.toml"),
            "[package]\nname = \"benchwork-core\"\n",
        );

        let owners = crate_owners(&[("benchwork".to_string(), root)]);

        assert_eq!(owners.get("benchwork"), Some(&"benchwork".to_string()));
        assert_eq!(owners.get("benchwork-core"), Some(&"benchwork".to_string()));
    }

    #[test]
    fn a_virtual_manifest_declares_no_package() {
        // A workspace-only manifest starts with [workspace]; reading its first `name` would attribute the
        // workspace itself to whichever crate key came first.
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("common");
        write(
            &root.join("Cargo.toml"),
            "[workspace]\nresolver = \"3\"\n\n[dependencies]\nname = \"not-a-package\"\n",
        );

        let owners = crate_owners(&[("common".to_string(), root)]);

        assert!(owners.is_empty(), "no package, no claim: {owners:?}");
    }

    #[test]
    fn a_repository_that_cannot_be_read_contributes_nothing() {
        let owners = crate_owners(&[(
            "ghost".to_string(),
            std::path::PathBuf::from("/nonexistent/member"),
        )]);
        assert!(owners.is_empty());
    }
}
