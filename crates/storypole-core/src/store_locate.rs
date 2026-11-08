//! Where a store lives, given the directory a command ran in (`GB-316`).
//!
//! The rules exist because three plausible answers disagree, and each disagreement was a defect:
//!
//! * The nearest `.gaugeblock/storypole` is not necessarily a store. A member repository carries a stub
//!   (an index and a `.gitignore`, no ledger), and opening it made `storypole verify` report
//!   `Task not found` from inside the repository the work lives in.
//! * The suite's ledger is the canonical one for member work (ADR 0001, ADR 0007), and it is only
//!   reachable through membership: `GaugeblockPaths::discover` answers with the member itself.
//! * A nested checkout that no suite lists must not adopt the ledger above it (`GB-305`).

use std::path::{Path, PathBuf};

use gaugeblock_workspace::{GaugeblockPaths, Membership};

use crate::error::StorypoleError;
use crate::store::StorypoleStore;

/// The `(root, dot_dir)` that a store opened from `start_dir` should use.
pub(crate) fn locate(start_dir: &Path) -> Result<(PathBuf, PathBuf), StorypoleError> {
    let enclosing = StorypoleStore::find_enclosing_store(start_dir);
    // The caller's own repository root, when git can say. A store above this line belongs to someone
    // else, and adopting it is how nested work lands in an unrelated ledger.
    let own_repo = gaugeblock_git::Repository::discover(start_dir)
        .ok()
        .map(|repo| repo.root().to_path_buf());
    let within_own_repo = |root: &Path| match &own_repo {
        Some(own) => root.starts_with(own),
        // No repository to compare against - a scratch directory, which storypole supports. The walk
        // found a store, so it is the only candidate.
        None => true,
    };

    // 1. A store inside the caller's own repository that actually holds a ledger. A member repository
    //    used on its own is this case, and its backlog is not the suite's business.
    if let Some((root, dot)) = &enclosing
        && has_ledger(dot)
        && within_own_repo(root)
    {
        return Ok((root.clone(), dot.clone()));
    }

    // 2. Inside a member of a suite: the canonical ledger is the suite's.
    if let Some(membership) = Membership::containing(start_dir) {
        let dot = GaugeblockPaths::at(&membership.suite_root).member("storypole");
        if dot.is_dir() {
            return Ok((membership.suite_root.clone(), dot));
        }
    }

    // 3. A store inside the caller's own repository with no ledger yet: a stub, or a directory worth
    //    initialising. Weaker than 1 and 2, but still the caller's own.
    if let Some((root, dot)) = &enclosing
        && within_own_repo(root)
    {
        return Ok((root.clone(), dot.clone()));
    }

    // 4. A store above the caller's own repository, and no suite claims it. Refuse rather than adopt.
    if let Some((root, dot)) = enclosing
        && within_own_repo(&root)
    {
        return Ok((root, dot));
    }

    Err(StorypoleError::Uninitialized(
        StorypoleStore::resolve_dot_dir(start_dir),
    ))
}

/// Whether this directory is a store or merely a place a tool wrote an index.
fn has_ledger(dot: &Path) -> bool {
    dot.join("ledger.jsonl").exists()
}
