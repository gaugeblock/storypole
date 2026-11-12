//! `config check --workspace` run from inside a member must audit the suite (`GB-305`).
//!
//! The debt this pays: `storypole-cli` calls `WorkspaceTopology::discover` on the directory it was launched
//! in. Launched at a suite root that is the suite. Launched inside a member it is the member, and
//! `--workspace` then audits a "workspace" of one repository that has no members at all - an answer that
//! looks like a pass and covers nothing. Every test that ran from a suite root agreed with the code, which
//! is the shape of every defect this ticket exists for.
//!
//! multi-repo: two member repositories under one suite root, and the command runs from a member.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_storypole");

/// A member repository: qualified for discovery by `.git`, and carrying its own configuration.
///
/// `.git` is a directory rather than a checkout because `WorkspaceTopology::discover` qualifies on its
/// presence, and building a real repository would test git rather than resolution. The configuration is
/// written in full because `StorypoleConfig` has no serde defaults - an empty table is not a config, and a
/// fixture that assumed otherwise would have failed for a reason unrelated to the behaviour under test.
fn member(root: &Path, name: &str) {
    let dir = root.join(name);
    fs::create_dir_all(dir.join(".git")).expect("create .git");
    let config_dir = dir.join(".gaugeblock").join("storypole");
    fs::create_dir_all(&config_dir).expect("create .gaugeblock/storypole");
    fs::write(
        config_dir.join("config.toml"),
        "ticket_prefix = \"AL\"\nallowed_repos = []\nallowed_crates = []\nclaim_ttl_minutes = 60\nverification_commands = []\n",
    )
    .expect("write member config");
}

/// A suite root holding `members`.
///
/// The suite declares its members, because that is what makes it a suite: two member directories under a
/// common parent is layout, and layout cannot tell a suite root from a member with subdirectories (`GB-324`).
fn suite(members: &[&str]) -> TempDir {
    let root = tempfile::tempdir().expect("temp dir");
    let suite_dir = root.path().join(".gaugeblock");
    fs::create_dir_all(&suite_dir).expect("create suite .gaugeblock");
    let declared: Vec<String> = members.iter().map(|name| format!("\"{name}\"")).collect();
    fs::write(
        suite_dir.join("config.toml"),
        format!("[suite]\nmembers = [{}]\n", declared.join(", ")),
    )
    .expect("write suite config");
    for name in members {
        member(root.path(), name);
    }
    root
}

/// Runs the built binary with `cwd`, returning (success, stdout, stderr).
fn run(cwd: &Path, args: &[&str]) -> (bool, String, String) {
    let output = Command::new(BIN)
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("storypole spawns");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

/// Parses the `repositories` count out of `config check --json`.
fn repositories(stdout: &str) -> u64 {
    let value: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("config check --json emits JSON");
    value["repositories"]
        .as_u64()
        .unwrap_or_else(|| panic!("no repositories count in {stdout}"))
}

#[test]
fn from_a_member_the_workspace_is_the_suite_not_the_member() {
    let root = suite(&["alpha", "beta"]);
    let member_dir: PathBuf = root.path().join("alpha");

    let (ok, stdout, stderr) = run(&member_dir, &["config", "check", "--workspace", "--json"]);
    assert!(
        ok,
        "config check --workspace failed from a member: {stderr}"
    );
    assert_eq!(
        repositories(&stdout),
        2,
        "launched in alpha, --workspace must audit the suite's two members, not alpha alone: {stdout}"
    );
}

#[test]
fn from_the_suite_root_the_answer_is_unchanged() {
    // The half that already worked, kept so the fix above cannot pay for itself by moving the boundary:
    // a command run at the suite root must still see both members.
    let root = suite(&["alpha", "beta"]);
    let (ok, stdout, stderr) = run(root.path(), &["config", "check", "--workspace", "--json"]);
    assert!(ok, "config check --workspace failed at the root: {stderr}");
    assert_eq!(repositories(&stdout), 2, "{stdout}");
}

#[test]
fn a_member_alone_is_still_a_workspace_of_one() {
    // Without a suite above it, a repository is its own workspace and says so. Widening the search must
    // not turn 'I am alone' into an error or into a claim about members that are not there.
    let standalone = tempfile::tempdir().expect("temp dir");
    member(standalone.path(), "solo");
    let solo = standalone.path().join("solo");

    let (ok, stdout, stderr) = run(&solo, &["config", "check", "--workspace", "--json"]);
    assert!(ok, "a lone repository must still audit itself: {stderr}");
    assert_eq!(repositories(&stdout), 1, "{stdout}");
}
