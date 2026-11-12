//! Multi-repository workspace management, discovery, and rollups.

pub mod config;
pub mod discovery;
pub mod lookup;
pub mod store;

pub use config::WorkspaceConfig;
pub use discovery::{DiscoveredMember, discover_members};
pub use lookup::{LocatedTask, find_task, suite_root_of};
pub use store::StorypoleWorkspaceStore;

/// Whether `root` declares `name` as a suite member (`GB-313`).
///
/// The distinction the verification guard needs: a scope naming a *declared member* that is not checked out
/// is a ticket whose chain would run in the wrong tree, while a scope naming the harness itself (`gaugeblock`,
/// `workspace`) has no directory by design - the root is that repository.
#[must_use]
pub fn declares_member(root: &std::path::Path, name: &str) -> bool {
    gaugeblock_workspace::SuiteConfig::load(root)
        .map(|config| config.members().iter().any(|member| member == name))
        .unwrap_or(false)
}
///
/// The roster is the `[suite] members` declaration and nothing else: a directory holding `.gaugeblock` is
/// not a suite for this question, because every member holds one too.
#[must_use]
pub fn declares_members(root: &std::path::Path) -> bool {
    gaugeblock_workspace::SuiteConfig::load(root)
        .map(|config| !config.members().is_empty())
        .unwrap_or(false)
}
