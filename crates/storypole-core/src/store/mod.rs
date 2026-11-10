//! Primary store facade managing `.storypole/` files, SQLite index, and configuration.

use crate::config::StorypoleConfig;
use crate::error::StorypoleError;
use crate::storage_mode::{StorageMode, sync_index};
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
    /// Which layout is authoritative for this store: one ledger, or one file per ticket.
    mode: StorageMode,
}

impl StorypoleStore {
    /// Resolves the storypole directory, preferring `.gaugeblock/storypole` if present,
    /// falling back to `.storypole`.
    ///
    /// The canonical candidate is spelled by `GaugeblockPaths` so the directory has one definition,
    /// but the *policy* here stays local on purpose: this must work in a scratch directory that is not
    /// a git repository, where `GaugeblockPaths::discover` deliberately refuses. Storypole stores are
    /// created in bare temp dirs and before the first commit, so resolution cannot require a checkout.
    pub fn resolve_dot_dir(root_dir: &Path) -> PathBuf {
        let canonical = gaugeblock_workspace::GaugeblockPaths::at(root_dir).member("storypole");
        if canonical.is_dir() {
            return canonical;
        }
        // Legacy only when it is actually there: an unconditional fallback created `.storypole` in every
        // repository that has `.gaugeblock` for other reasons, which misfiled seven tickets (`GB-331`).
        let legacy = root_dir.join(".storypole");
        if legacy.is_dir() {
            return legacy;
        }
        canonical
    }

    /// Discover the nearest enclosing repository containing `.gaugeblock/storypole` or `.storypole`,
    /// traversing up parent directories starting from `start_dir`.
    pub fn find_enclosing_store(start_dir: &Path) -> Option<(PathBuf, PathBuf)> {
        for ancestor in start_dir.ancestors() {
            let gb_dir = gaugeblock_workspace::GaugeblockPaths::at(ancestor).member("storypole");
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
    ///
    /// Where the store is, and which of several plausible stores wins, is [`crate::store_locate`]'s
    /// decision: it is the only place that knows a stub from a ledger and the suite from its members.
    pub fn open(start_dir: &Path) -> Result<Self, StorypoleError> {
        let (root_dir, dot_dir) = crate::store_locate::locate(start_dir)?;

        let config_path = dot_dir.join("config.toml");

        let loaded =
            if let Ok(cfg) = StorypoleConfig::load_unified_or_legacy(&root_dir, &config_path) {
                cfg
            } else if !config_path.exists() {
                return Err(StorypoleError::Uninitialized(dot_dir));
            } else {
                StorypoleConfig::load_from_file(&config_path)?
            };

        // A member repository declares no chains of its own: they live once at the suite root, keyed by the
        // directory names the `[suite] members` roster uses. Without this inheritance a member-scoped task asked
        // its own config for a chain, found none, and fell back to `cargo test` - a weaker certification than the
        // configuration promised, recorded honestly and still wrong (`SP-1`).
        let config = loaded.inheriting(chains_declared_by_suite(&root_dir));

        let sqlite_path = dot_dir.join("index.sqlite");
        let (conn, mode) = sync_index(&sqlite_path, &dot_dir)?;

        Ok(Self {
            root_dir,
            dot_dir,
            config,
            conn,
            mode,
        })
    }

    /// Open an existing storypole repository strictly at `root_dir` without upward traversal.
    pub fn open_exact(root_dir: &Path) -> Result<Self, StorypoleError> {
        let dot_dir = Self::resolve_dot_dir(root_dir);
        let config_path = dot_dir.join("config.toml");

        let loaded =
            if let Ok(cfg) = StorypoleConfig::load_unified_or_legacy(root_dir, &config_path) {
                cfg
            } else if !config_path.exists() {
                return Err(StorypoleError::Uninitialized(dot_dir));
            } else {
                StorypoleConfig::load_from_file(&config_path)?
            };

        // Same inheritance as `open`, for the caller that pinned the directory instead of searching (`SP-1`).
        let config = loaded.inheriting(chains_declared_by_suite(root_dir));

        let sqlite_path = dot_dir.join("index.sqlite");
        let (conn, mode) = sync_index(&sqlite_path, &dot_dir)?;

        Ok(Self {
            root_dir: root_dir.to_path_buf(),
            dot_dir,
            config,
            conn,
            mode,
        })
    }

    /// Initialize a new storypole repository at `root_dir`.
    ///
    /// Always `.gaugeblock/storypole`, never the legacy `.storypole`, because ADR 0001 says canonical state
    /// lives in `.gaugeblock` and a default that disagrees with the architecture is a trap for every new
    /// repository (`GB-344`). It used to fall back to `.storypole` whenever `.gaugeblock` did not exist yet,
    /// which is precisely the case in a repository being initialised.
    ///
    /// Existing `.storypole` stores keep working: [`Self::resolve_dot_dir`] still prefers a
    /// `.gaugeblock/storypole` that exists and still reads one that is legacy.
    pub fn init(root_dir: &Path, config: StorypoleConfig) -> Result<Self, StorypoleError> {
        let paths = gaugeblock_workspace::GaugeblockPaths::at(root_dir);
        let dot_dir = paths.member("storypole");
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
        let (conn, mode) = sync_index(&sqlite_path, &dot_dir)?;

        Ok(Self {
            root_dir: root_dir.to_path_buf(),
            dot_dir,
            config,
            conn,
            mode,
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

    /// Which layout this store reads and writes.
    pub fn storage_mode(&self) -> StorageMode {
        self.mode
    }

    /// Current Unix epoch timestamp in milliseconds.
    pub fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

mod writes;

/// The verification chains declared by the suite that contains `root_dir` (`SP-1`).
///
/// Returns nothing for a lone checkout, which is why the caller can inherit unconditionally: a repository that is
/// not in a suite keeps exactly the configuration it had before. A malformed suite config also yields nothing -
/// the suite's own tooling is the right place to complain about it, and a member should not refuse to open because
/// a file it does not own is broken.
fn chains_declared_by_suite(root_dir: &Path) -> Vec<(String, String)> {
    let Some(membership) = gaugeblock_workspace::Membership::containing(root_dir) else {
        return Vec::new();
    };
    let path = gaugeblock_workspace::GaugeblockPaths::at(&membership.suite_root).config();
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(table) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let Some(entries) = table
        // The chains live under `[storypole]`, not at the top level: the suite config opens that section a few
        // lines above `verification_commands`. Reading the root table - which the first version of this function
        // did - finds nothing and inherits nothing, silently, which is the same wrong-place lookup this ticket was
        // filed about (`SP-1`).
        .get("storypole")
        .and_then(|value| value.as_table())
        .and_then(|section| section.get("verification_commands"))
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };
    // Each entry is `[key, command]`. Anything else is skipped rather than fatal: the caller falls back to the
    // previous behaviour, which is a chain that does not run - the status quo, not a new failure.
    entries
        .iter()
        .filter_map(|entry| {
            let pair = entry.as_array()?;
            let key = pair.first()?.as_str()?;
            let command = pair.get(1)?.as_str()?;
            Some((key.to_string(), command.to_string()))
        })
        .collect()
}
