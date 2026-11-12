//! Where a verification chain runs, and whether that is legitimate (`SP-50`, `GB-313`).
//!
//! Two questions a receipt must answer honestly: which repository the chain ran in, and whether the ticket
//! was allowed to be verified there at all. Both were answered wrong in ways nobody could see - a harness
//! chain silently standing in for a member's, and a receipt naming a repository while the chain ran
//! against another tree.

use crate::error::StorypoleError;
use crate::store::StorypoleStore;

/// The repository that owns this task's crate, when the task names only a crate (`SP-50`).
///
/// The chain lookup used to answer "no chain" for a task scoped `crate_name: storypole-cli` with no
/// repository, even though `storypole` declares that crate and configures a full chain. The receipt then
/// certified `cargo test` and could not say which repository it had measured.
///
/// Evidence, not a guess: the answer comes from `[package] name` in the member manifests, the same map the
/// ticket migration adopted work by. A crate no member declares returns `None`, and the caller keeps the
/// honest fallback.
pub(super) fn owning_repo(store: &StorypoleStore, crate_name: &str) -> Option<String> {
    // Canonicalised before asking membership, because `Membership::containing` walks ancestors and a
    // relative path stops at the current directory (the `OF-42` lesson).
    let root = store.root_dir();
    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    // A member's store answers for its suite; a lone checkout answers for itself.
    let suite_root = gaugeblock_workspace::Membership::containing(&canonical)
        .map_or_else(|| canonical.clone(), |membership| membership.suite_root);
    let topology = gaugeblock_workspace::WorkspaceTopology::discover(&suite_root);
    let members: Vec<(String, std::path::PathBuf)> = topology
        .members()
        .iter()
        .map(|member| (member.name.clone(), member.path.clone()))
        .collect();
    crate::migrate::manifests::crate_owners(&members)
        .get(crate_name)
        .cloned()
}

/// Refuses a chain that would run against a tree the ticket never meant (`GB-313`).
///
/// `resolve_target_dir` falls back to the store root when `root/<repo>` is absent, and the receipt then
/// stamped the *declared* repository - so the record named a tree nothing ran against. The guard is
/// deliberately limited to *declared members*: a scope naming the harness itself (`gaugeblock`,
/// `workspace`) has no directory by design, because the root is that repository.
pub(super) fn ensure_member_checked_out(
    store: &StorypoleStore,
    task: &crate::model::TaskNode,
) -> Result<(), StorypoleError> {
    let Some(repo) = task.scope.repo.as_deref() else {
        return Ok(());
    };
    let expected = store.root_dir().join(repo);
    if expected.is_dir() || !crate::workspace::declares_member(store.root_dir(), repo) {
        return Ok(());
    }
    Err(StorypoleError::RepositoryNotCheckedOut {
        ticket: task.ticket_key.clone(),
        repo: repo.to_string(),
        path: expected,
    })
}
