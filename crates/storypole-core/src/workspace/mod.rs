//! Multi-repository workspace management, discovery, and rollups.

pub mod config;
pub mod discovery;
pub mod lookup;
pub mod store;

pub use config::WorkspaceConfig;
pub use discovery::{DiscoveredMember, discover_members};
pub use lookup::{LocatedTask, find_task, suite_root_of};
pub use store::StorypoleWorkspaceStore;

/// Whether `root` declares suite members, which is what makes its answers a *suite* view (`GB-355`).
///
/// The roster is the `[suite] members` declaration and nothing else: a directory holding `.gaugeblock` is
/// not a suite for this question, because every member holds one too.
#[must_use]
pub fn declares_members(root: &std::path::Path) -> bool {
    gaugeblock_workspace::SuiteConfig::load(root)
        .map(|config| !config.members().is_empty())
        .unwrap_or(false)
}
