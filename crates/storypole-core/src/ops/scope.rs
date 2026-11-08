//! Which member repository a task belongs to, and how that was decided (`GB-316`).
//!
//! `TaskScope.repo` existed from the start and no path ever populated it, so every consumer that
//! needed a repository fell through to the harness: a receipt for member work named the harness
//! branch, and `verify` ran the harness chain against a member ticket.
//!
//! Two rules govern this module.
//!
//! * **A guessed value must be recorded and visible.** Every answer carries the [`ScopeSource`] that
//!   produced it, so a receipt can say whether the repository was given or derived.
//! * **An absent value must not resolve to a default.** Derivation ends at [`ScopeSource::Harness`],
//!   which is a statement about the ticket rather than a fallback for failed detection.

use std::path::Path;

/// How a repository scope was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeSource {
    /// Given on the command line or through MCP.
    Explicit,
    /// Already recorded on the task.
    Persisted,
    /// Derived from the paths the task names.
    Files,
    /// Derived from the crate named by the task's symbols.
    Symbols,
    /// Derived from the directory the command was run in.
    WorkingDirectory,
    /// Nothing named a repository: harness-level work, stated rather than assumed.
    Harness,
}

impl ScopeSource {
    /// A short name for output and receipts.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::Persisted => "persisted",
            Self::Files => "files",
            Self::Symbols => "symbols",
            Self::WorkingDirectory => "working-directory",
            Self::Harness => "harness",
        }
    }
}

/// A resolved repository scope together with how it was resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedScope {
    /// The member repository name, or `None` for harness-level work.
    pub repo: Option<String>,
    /// Which rule produced [`ResolvedScope::repo`].
    pub source: ScopeSource,
}

impl ResolvedScope {
    fn from_source(repo: Option<String>, source: ScopeSource) -> Self {
        Self { repo, source }
    }
}

/// Resolves the repository a task belongs to.
///
/// The order is fixed: an explicit argument, then the persisted scope, then the paths the task names,
/// then the crate its symbols name, then the working directory. `members` is the suite's member list;
/// an answer that is not a member is not an answer.
#[must_use]
pub fn resolve_scope(
    explicit: Option<&str>,
    persisted: Option<&str>,
    files: &[String],
    symbols: &[String],
    members: &[String],
    caller_dir: Option<&Path>,
    suite_root: &Path,
) -> ResolvedScope {
    if let Some(repo) = explicit.map(str::trim).filter(|r| !r.is_empty()) {
        return ResolvedScope::from_source(Some(repo.to_string()), ScopeSource::Explicit);
    }
    if let Some(repo) = persisted.map(str::trim).filter(|r| !r.is_empty()) {
        return ResolvedScope::from_source(Some(repo.to_string()), ScopeSource::Persisted);
    }
    if let Some(repo) = from_files(files, members) {
        return ResolvedScope::from_source(Some(repo), ScopeSource::Files);
    }
    if let Some(repo) = from_symbols(symbols, members) {
        return ResolvedScope::from_source(Some(repo), ScopeSource::Symbols);
    }
    if let Some(repo) = from_caller_dir(caller_dir, members, suite_root) {
        return ResolvedScope::from_source(Some(repo), ScopeSource::WorkingDirectory);
    }
    ResolvedScope::from_source(None, ScopeSource::Harness)
}

/// The member that owns the first path which names one.
///
/// A path has to begin with the member's directory; a substring match would place
/// `docs/storypole-notes.md` in `storypole`.
fn from_files(files: &[String], members: &[String]) -> Option<String> {
    for file in files {
        let normalized = file.trim_start_matches("./").replace('\\', "/");
        let first = normalized.split('/').next()?;
        if let Some(member) = members.iter().find(|m| m.as_str() == first) {
            return Some(member.clone());
        }
    }
    None
}

/// The member that owns the crate a symbol is defined in.
///
/// The crate name is taken before `::`, underscores normalised to hyphens, and matched against a
/// member by whole segment: `storypole_core::x` places in `storypole`, and `miter-cli` in `miter`.
///
/// A crate that does not begin with its repository's name cannot be placed this way. `common`'s
/// crates are the standing example - `gaugeblock-contracts` starts with the facade's name, not the
/// member's - so symbols alone answer for those only through their file paths.
fn from_symbols(symbols: &[String], members: &[String]) -> Option<String> {
    for symbol in symbols {
        let path = symbol.split("::").next().unwrap_or(symbol);
        let normalized = path.trim().replace('_', "-").to_ascii_lowercase();
        let mut segments: Vec<&str> = normalized.split('-').collect();
        // Drop the trailing segments until a member name is consumed, longest first.
        while !segments.is_empty() {
            let candidate = segments.join("-");
            if let Some(member) = members.iter().find(|m| m.as_str() == candidate) {
                return Some(member.clone());
            }
            segments.pop();
        }
    }
    None
}

/// The member whose directory contains the caller.
fn from_caller_dir(
    caller_dir: Option<&Path>,
    members: &[String],
    suite_root: &Path,
) -> Option<String> {
    let caller = caller_dir?;
    for member in members {
        if caller.starts_with(suite_root.join(member)) {
            return Some(member.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests;
