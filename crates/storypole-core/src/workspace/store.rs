//! Workspace store facade providing unified rollups and routing across member repositories.

use crate::error::StorypoleError;
use crate::index::open_and_sync_index;
use crate::ledger::read_events;
use crate::ledger::replay::replay_events;
use crate::store::StorypoleStore;
use crate::workspace::config::WorkspaceConfig;
use crate::workspace::discovery::{DiscoveredMember, discover_members};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Unified multi-repository workspace store.
#[derive(Debug)]
pub struct StorypoleWorkspaceStore {
    workspace_root: PathBuf,
    config: Option<WorkspaceConfig>,
    members: Vec<DiscoveredMember>,
    member_stores: HashMap<String, StorypoleStore>,
    root_store: Option<StorypoleStore>,
    unified_conn: Connection,
}

impl StorypoleWorkspaceStore {
    /// Open a multi-repo workspace at `workspace_root`.
    pub fn open(workspace_root: &Path) -> Result<Self, StorypoleError> {
        let ws_config = WorkspaceConfig::load_from_root(workspace_root)?;
        let members = discover_members(workspace_root, ws_config.as_ref());

        let root_store = StorypoleStore::open_exact(workspace_root).ok();

        let mut member_stores = HashMap::new();
        for m in &members {
            if m.dot_dir.is_some()
                && let Ok(store) = StorypoleStore::open_exact(&m.repo_path)
            {
                member_stores.insert(m.name.clone(), store);
            }
        }

        // Use root store's SQLite index if available, otherwise ephemeral in-memory index
        let unified_conn = if let Some(ref rs) = root_store {
            open_and_sync_index(&rs.sqlite_path(), &rs.ledger_path())?
        } else {
            let conn = Connection::open_in_memory().map_err(StorypoleError::Sqlite)?;
            crate::index::init_schema(&conn)?;
            conn
        };

        let mut ws = Self {
            workspace_root: workspace_root.to_path_buf(),
            config: ws_config,
            members,
            member_stores,
            root_store,
            unified_conn,
        };

        ws.sync_all_members()?;
        Ok(ws)
    }

    /// Synchronize all member stores into the unified SQLite cache.
    pub fn sync_all_members(&mut self) -> Result<(), StorypoleError> {
        let mut all_events = Vec::new();

        if let Some(ref rs) = self.root_store
            && let Ok(events) = read_events(&rs.ledger_path())
        {
            all_events.extend(events);
        }

        for store in self.member_stores.values() {
            if let Ok(events) = read_events(&store.ledger_path()) {
                all_events.extend(events);
            }
        }

        let tasks = replay_events(&all_events);
        let tx = self
            .unified_conn
            .transaction()
            .map_err(StorypoleError::Sqlite)?;
        tx.execute("DELETE FROM tasks", [])
            .map_err(StorypoleError::Sqlite)?;
        tx.execute("DELETE FROM task_labels", [])
            .map_err(StorypoleError::Sqlite)?;
        tx.execute("DELETE FROM task_files", [])
            .map_err(StorypoleError::Sqlite)?;
        tx.execute("DELETE FROM task_symbols", [])
            .map_err(StorypoleError::Sqlite)?;
        tx.execute("DELETE FROM task_edges", [])
            .map_err(StorypoleError::Sqlite)?;

        for task in tasks.values() {
            crate::index::insert_task_record(&tx, task)?;
        }
        tx.commit().map_err(StorypoleError::Sqlite)?;
        Ok(())
    }

    /// Workspace configuration, if loaded from root.
    pub fn config(&self) -> Option<&WorkspaceConfig> {
        self.config.as_ref()
    }

    /// Access active unified SQLite database connection.
    pub fn conn(&self) -> &Connection {
        &self.unified_conn
    }

    /// Mutable access to unified SQLite database connection.
    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.unified_conn
    }

    /// Discovered member repositories.
    pub fn members(&self) -> &[DiscoveredMember] {
        &self.members
    }

    /// Workspace root directory.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Retrieve a member store by repository name.
    pub fn member_store_mut(&mut self, repo_name: &str) -> Option<&mut StorypoleStore> {
        self.member_stores.get_mut(repo_name)
    }

    /// Access root store, if initialized.
    pub fn root_store_mut(&mut self) -> Option<&mut StorypoleStore> {
        self.root_store.as_mut()
    }

    /// Find which store governs a given ticket key or task UUID.
    pub fn resolve_governing_store_name(&self, identifier: &str) -> Option<String> {
        let lower = identifier.to_lowercase();
        for m in &self.members {
            if let Some(ref prefix) = m.ticket_prefix {
                let p_dash = format!("{}-", prefix.to_lowercase());
                if lower.starts_with(&p_dash) {
                    return Some(m.name.clone());
                }
            }
        }

        // Query database to check task's repo
        let mut stmt = self
            .unified_conn
            .prepare("SELECT repo FROM tasks WHERE ticket_key_lower = ? OR id = ?")
            .ok()?;
        let repo: Option<String> = stmt
            .query_row(rusqlite::params![lower, identifier], |r| r.get(0))
            .ok()
            .flatten();

        repo
    }
}
