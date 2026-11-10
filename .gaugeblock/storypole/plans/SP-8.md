# Task Plan: GB-132 — Configure SQLite WAL Journal Mode and synchronous=NORMAL in storypole-core

> **Task Key:** `GB-132`  
> **Parent Epic:** `GB-128`  
> **Scope:** `storypole-core`  
> **Status:** Implemented — Tier 2 green, pending Tier 3

---

## 1. Problem Statement
`storypole-core` currently opens `index.sqlite` without setting SQLite performance PRAGMAs, defaulting to rollback journal deletion and full synchronous disk flushes (`PRAGMA synchronous = FULL`). Every task update, verification, and ledger rebuild incurs blocking `fsync()` system calls.

---

## 2. Technical Blueprint
1. In `storypole-core/src/index/mod.rs` (`open_and_sync_index`):
   * Immediately after `Connection::open(sqlite_path)`, execute:
     ```rust
     conn.execute_batch(
         "PRAGMA journal_mode = WAL;
          PRAGMA synchronous = NORMAL;
          PRAGMA temp_store = MEMORY;
          PRAGMA cache_size = -64000;
          PRAGMA busy_timeout = 5000;"
     )?;
     ```
2. Verify WAL mode compatibility with existing tests and multi-process access.

---

## 3. Execution Steps
* **Step 1:** Add high-performance SQLite PRAGMAs to `open_and_sync_index` in `storypole-core`.
* **Step 2:** Verify all storypole unit and integration tests pass cleanly under WAL mode.
* **Step 3:** Tier 2 verification (`cargo nextest run -p storypole-core`).
