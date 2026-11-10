//! Workspace store facade providing unified rollups and routing across member repositories.

use crate::error::StorypoleError;
use crate::ledger::read_events;
use crate::ledger::replay::replay_events;
use crate::model::LedgerEvent;
use crate::storage_mode::StorageMode;
use crate::store::StorypoleStore;
use crate::workspace::config::WorkspaceConfig;
use crate::workspace::discovery::{DiscoveredMember, discover_members};
use rusqlite::Connection;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use uuid::Uuid;

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

        // The unified view gets its OWN file. Sharing the root store's `index.sqlite` meant a store-scoped command
        // rebuilt it from the root ledger alone, truncating the suite to the harness and leaving a board to read
        // whichever command ran last (`GB-353`). `sync_all_members` fills this one from every store below.
        let unified_conn = if let Some(ref rs) = root_store {
            crate::index::open_index(&rs.workspace_sqlite_path())?
        } else {
            let conn = Connection::open_in_memory().map_err(StorypoleError::Sqlite)?;
            crate::index::apply_connection_pragmas(&conn)?;
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
    ///
    /// Each source is read in whichever layout it uses (`GB-330`): a repository with ticket records is
    /// replayed from its files, and one that still has a ledger is read from the ledger. Two sources holding
    /// the same task id is refused rather than merged - one ticket lives in one file, and quietly picking a
    /// winner is what made `GB-79`'s copies disagree about whether work was done.
    pub fn sync_all_members(&mut self) -> Result<(), StorypoleError> {
        let mut all_events = Vec::new();
        let mut sources: Vec<(String, &StorypoleStore)> = Vec::new();
        if let Some(ref rs) = self.root_store {
            sources.push(("the workspace root".to_string(), rs));
        }
        for (name, store) in &self.member_stores {
            sources.push((name.clone(), store));
        }
        // Members come from a HashMap, so without this the duplicate-task message names the same pair in a
        // different order on every run - a report that changes wording is a report people stop trusting.
        sources.sort_by(|a, b| a.0.cmp(&b.0));

        let mut seen: BTreeMap<Uuid, String> = BTreeMap::new();
        for (label, store) in &sources {
            for id in task_ids(store)? {
                if let Some(first) = seen.insert(id, label.clone()) {
                    return Err(StorypoleError::TicketRecord {
                        path: store.dot_dir().to_path_buf(),
                        reason: format!(
                            "task {id} is recorded in both {first} and {label}; one ticket belongs in one file"
                        ),
                    });
                }
            }
            all_events.extend(events_of(store)?);
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

/// Every event a store owns, from whichever layout it uses.
fn events_of(store: &StorypoleStore) -> Result<Vec<LedgerEvent>, StorypoleError> {
    match store.storage_mode() {
        StorageMode::Records => {
            let mut events = Vec::new();
            for record in crate::tickets::read_all(store.dot_dir())? {
                events.extend(record.events);
            }
            Ok(events)
        }
        StorageMode::Ledger => read_events(&store.ledger_path()),
    }
}

/// The task ids a store owns, which is what makes a duplicate detectable before anything is merged.
fn task_ids(store: &StorypoleStore) -> Result<Vec<Uuid>, StorypoleError> {
    match store.storage_mode() {
        StorageMode::Records => Ok(crate::tickets::read_all(store.dot_dir())?
            .into_iter()
            .map(|record| record.ticket.id)
            .collect()),
        StorageMode::Ledger => Ok(replay_events(&read_events(&store.ledger_path())?)
            .into_keys()
            .collect()),
    }
}
