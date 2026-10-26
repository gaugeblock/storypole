//! Multi-repository workspace management, discovery, and rollups.

pub mod config;
pub mod discovery;
pub mod store;

pub use config::WorkspaceConfig;
pub use discovery::{DiscoveredMember, discover_members};
pub use store::StorypoleWorkspaceStore;

