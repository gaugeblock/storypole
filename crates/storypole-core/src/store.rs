//! Primary store facade managing `.storypole/` files, SQLite index, and configuration.

use crate::config::StorypoleConfig;
use crate::error::StorypoleError;
use crate::index::open_and_sync_index;
use crate::ledger::append_event;
use crate::model::LedgerEvent;
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Central gateway for storypole repository access, caching, and mutations.
#[derive(Debug)]
pub struct StorypoleStore {
    root_dir: PathBuf,
    dot_dir: PathBuf,
    config: StorypoleConfig,
    conn: Connection,
}

impl StorypoleStore {
    /// Resolves the storypole directory, preferring `.gaugeblock/storypole` if present,
    /// falling back to `.storypole`.
    pub fn resolve_dot_dir(root_dir: &Path) -> PathBuf {
        let gb_dir = root_dir.join(".gaugeblock").join("storypole");
        if gb_dir.is_dir() {
            gb_dir
        } else {
            root_dir.join(".storypole")
        }
    }

    /// Discover the nearest enclosing repository containing `.gaugeblock/storypole` or `.storypole`,
    /// traversing up parent directories starting from `start_dir`.
    pub fn find_enclosing_store(start_dir: &Path) -> Option<(PathBuf, PathBuf)> {
        for ancestor in start_dir.ancestors() {
            let gb_dir = ancestor.join(".gaugeblock").join("storypole");
            if gb_dir.is_dir() {
                return Some((ancestor.to_path_buf(), gb_dir));
            }
            let sp_dir = ancestor.join(".storypole");
            if sp_dir.is_dir() {
                return Some((ancestor.to_path_buf(), sp_dir));
            }
        }
        None
    }

    /// Open an existing storypole repository at or above `start_dir`.
    pub fn open(start_dir: &Path) -> Result<Self, StorypoleError> {
        let (root_dir, dot_dir) = match Self::find_enclosing_store(start_dir) {
            Some(pair) => pair,
            None => {
                let fallback = Self::resolve_dot_dir(start_dir);
                return Err(StorypoleError::Uninitialized(fallback));
            }
        };

        let config_path = dot_dir.join("config.toml");

        let config = if let Ok(cfg) = StorypoleConfig::load_unified_or_legacy(&root_dir, &config_path) {
            cfg
        } else if !config_path.exists() {
            return Err(StorypoleError::Uninitialized(dot_dir));
        } else {
            StorypoleConfig::load_from_file(&config_path)?
        };

        let ledger_path = dot_dir.join("ledger.jsonl");
        let sqlite_path = dot_dir.join("index.sqlite");

        let conn = open_and_sync_index(&sqlite_path, &ledger_path)?;

        Ok(Self {
            root_dir,
            dot_dir,
            config,
            conn,
        })
    }

    /// Open an existing storypole repository strictly at `root_dir` without upward traversal.
    pub fn open_exact(root_dir: &Path) -> Result<Self, StorypoleError> {
        let dot_dir = Self::resolve_dot_dir(root_dir);
        let config_path = dot_dir.join("config.toml");

        let config = if let Ok(cfg) = StorypoleConfig::load_unified_or_legacy(root_dir, &config_path) {
            cfg
        } else if !config_path.exists() {
            return Err(StorypoleError::Uninitialized(dot_dir));
        } else {
            StorypoleConfig::load_from_file(&config_path)?
        };

        let ledger_path = dot_dir.join("ledger.jsonl");
        let sqlite_path = dot_dir.join("index.sqlite");

        let conn = open_and_sync_index(&sqlite_path, &ledger_path)?;

        Ok(Self {
            root_dir: root_dir.to_path_buf(),
            dot_dir,
            config,
            conn,
        })
    }

    /// Initialize a new storypole repository at `root_dir`.
    pub fn init(root_dir: &Path, config: StorypoleConfig) -> Result<Self, StorypoleError> {
        let dot_dir = if root_dir.join(".gaugeblock").is_dir() {
            root_dir.join(".gaugeblock").join("storypole")
        } else {
            root_dir.join(".storypole")
        };
        fs::create_dir_all(&dot_dir).map_err(|e| StorypoleError::IoWithPath {
            path: dot_dir.clone(),
            source: e,
        })?;

        let config_path = dot_dir.join("config.toml");
        config.save_to_file(&config_path)?;

        let gitignore_path = dot_dir.join(".gitignore");
        if !gitignore_path.exists() {
            let ignore_content = "index.sqlite\nindex.sqlite-shm\nindex.sqlite-wal\n";
            fs::write(&gitignore_path, ignore_content).map_err(|e| StorypoleError::IoWithPath {
                path: gitignore_path.clone(),
                source: e,
            })?;
        }

        let ledger_path = dot_dir.join("ledger.jsonl");
        if !ledger_path.exists() {
            fs::File::create(&ledger_path).map_err(|e| StorypoleError::IoWithPath {
                path: ledger_path.clone(),
                source: e,
            })?;
        }

        let sqlite_path = dot_dir.join("index.sqlite");
        let conn = open_and_sync_index(&sqlite_path, &ledger_path)?;

        Ok(Self {
            root_dir: root_dir.to_path_buf(),
            dot_dir,
            config,
            conn,
        })
    }

    /// Access active repository configuration.
    pub fn config(&self) -> &StorypoleConfig {
        &self.config
    }

    /// Mutable access to SQLite database connection.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Mutable access to SQLite database connection.
    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Root directory of the repository containing `.storypole`.
    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    /// Path to the `.storypole` directory.
    pub fn dot_dir(&self) -> &Path {
        &self.dot_dir
    }

    /// Path to `ledger.jsonl`.
    pub fn ledger_path(&self) -> PathBuf {
        self.dot_dir.join("ledger.jsonl")
    }

    /// Path to `index.sqlite`.
    pub fn sqlite_path(&self) -> PathBuf {
        self.dot_dir.join("index.sqlite")
    }

    /// Record a ledger event, sync SQLite index, and persist to disk.
    pub fn record_event(&mut self, event: &LedgerEvent) -> Result<(), StorypoleError> {
        let ledger = self.ledger_path();
        append_event(&ledger, event)?;
        let sqlite = self.sqlite_path();
        self.conn = open_and_sync_index(&sqlite, &ledger)?;
        Ok(())
    }

    /// Current Unix epoch timestamp in milliseconds.
    pub fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}
