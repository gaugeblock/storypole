# GB-57: First-Class Multi-Repository Workspace Support and Distributed Ledgers for Storypole

> **Status:** Ready  
> **Target Scope:** `storypole` (`storypole-core`, `storypole-cli`, `storypole-mcp`)  
> **Key Invariants:**  
> 1. Hard 250 LOC rule on all `.rs` files.  
> 2. Zero-panic safety: no `.unwrap()` / `.expect()` in production library paths (`thiserror`).  
> 3. Unix Philosophy & Standalone Autonomy: every sub-repository maintains its own `.storypole/` directory, ledger, and plans versioned directly in its own Git repository.  
> 4. Non-breaking compatibility: standalone repos without workspaces continue to function identically.  
> 5. Deterministic cross-repo DAG links using UUIDs with cached external ticket keys and repo names.  

---

## 1. Executive Summary & Problem Formulation

`storypole` currently assumes a single-repository model per store instance, resolving `.gaugeblock/storypole` or `.storypole` strictly relative to the current working directory (`env::current_dir()`) without upward directory traversal.

In the meta-harness (`neurosymbolic-suite`), all backlog items are written to a single root ledger (`.gaugeblock/storypole/ledger.jsonl`) under the `GB` prefix, even if their `TaskScope` targets specific sub-repositories (`repo: "benchwork"`, `repo: "miter"`, etc.).

Sub-repositories (`benchwork`, `miter`, `chalkline`, `dado`, `batten`, `joiner`, `outfeed`, `rabbet`, `sawhorse`, `storypole`) are standalone Git repositories and Cargo workspaces. Currently, none of them contain a local `.storypole/` directory. If a developer or downstream user clones only `benchwork` or `miter` as a standalone project and launches `storypole list` or `benchwork`, they see an uninitialized store error. None of the tickets, execution plans (`plans/<KEY>.md`), or architectural decisions for that repo exist within the cloned repository's Git history.

`GB-57` transforms `storypole` into a **hybrid distributed-ledger task engine** with first-class multi-repo workspace support:
1. **Standalone Autonomy:** Every sub-repository owns its own `.storypole/` directory, ledger (`ledger.jsonl`), plans (`plans/`), and local SQLite index (`index.sqlite`), versioned directly in its own Git repository with a dedicated ticket prefix (`BW-1`, `MTR-1`, `JN-1`, etc.).
2. **First-Class Multi-Repo Rollup:** When invoked from a meta-harness root, `storypole` rolls up tasks, decisions, and DAG topology across all member repositories alongside meta-level workspace tickets (`GB-1`).
3. **Smart Context & Scope Routing:** CLI and MCP write operations automatically route to the appropriate repository ledger based on working directory, `--repo` flag, ticket prefix, or interactive prompt.
4. **Cross-Repo DAG Topological Links:** A task in `joiner` can depend on a task in `miter` via deterministic UUIDs, gracefully rendering in both workspace-wide DAG scheduling and standalone external-dependency views.
5. **Automated Migration (`storypole workspace distribute`):** Migrates existing scoped tasks from root ledger to child repos while preserving UUIDs and aliasing legacy keys.

---

## 2. Architecture & File Layout

```text
storypole/crates/storypole-core/src/
├── workspace/
│   ├── mod.rs               # Workspace discovery, config loader, and Store resolver (<200 LOC)
│   ├── config.rs            # WorkspaceConfig schema [workspace] members = [...]
│   ├── discovery.rs         # Directory scan, git/storypole detection
│   └── rollup.rs            # Unified SQLite aggregation and query proxy
├── store/
│   ├── mod.rs               # StorypoleStore definition (<200 LOC)
│   ├── traversal.rs         # Upward directory search for .storypole / .gaugeblock
│   └── routing.rs           # Mutation target routing (repo vs workspace)
├── model/
│   ├── task.rs              # Extended TaskEdge with target_ticket_key & target_repo
│   └── ...
└── index/
    ├── query.rs             # Multi-prefix get_max_ticket_number and resolvers
    └── ...

storypole/crates/storypole-cli/src/
├── handlers/
│   ├── workspace_cmds.rs    # workspace init, status, and distribute handlers (<200 LOC)
│   └── ...
└── args.rs                  # Clap definitions for workspace subcommands and --repo routing
```

---

## 3. Data Contracts & TOML Schemas

### 3.1 Workspace Configuration (`[workspace]` table in root `config.toml`)
```toml
[workspace]
members = [
    "batten",
    "benchwork",
    "chalkline",
    "dado",
    "joiner",
    "miter",
    "outfeed",
    "rabbet",
    "sawhorse",
    "storypole",
]
default_prefix = "GB"
```

### 3.2 Member Repository Configuration (`<repo>/.storypole/config.toml`)
```toml
ticket_prefix = "BW"
allowed_repos = ["benchwork"]
allowed_crates = ["benchwork-core", "benchwork-cli", "benchwork-mcp"]
claim_ttl_minutes = 60
verification_commands = [
    ["benchwork-core", "cargo test -p benchwork-core"],
]
```

### 3.3 Extended DAG Edge Contract (`TaskEdge`)
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskEdge {
    pub kind: EdgeKind,
    pub target_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_ticket_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_repo: Option<String>,
}
```

---

## 4. Execution Plan & Verification Gates

### Step 1: Upward Directory Traversal & Multi-Prefix Resolution
* Implement recursive upward search for `.storypole` or `.gaugeblock/storypole` in `StorypoleStore::resolve_dot_dir`.
* Update `get_max_ticket_number` and `resolve_task_id` to cleanly support arbitrary repo prefixes simultaneously in SQLite.
* *Verification Gate:* Unit tests in `storypole-core` asserting upward traversal from arbitrary nested subdirectories, and resolving tasks across different prefixes.

### Step 2: Workspace Discovery & Configuration Schema
* Define `WorkspaceConfig` schema and parser.
* Implement filesystem auto-discovery for sibling/child directories containing `.storypole` or `.git`.
* *Verification Gate:* Tests for glob discovery, explicit member lists, and fallback auto-detection.

### Step 3: Cross-Repo DAG Edges & Formatting
* Extend `TaskEdge` with optional `target_ticket_key` and `target_repo`.
* Update DAG briefing and topological sort to handle external unresolved nodes gracefully without panicking.
* *Verification Gate:* Round-trip JSON serde test for `TaskEdge` and DAG traversal test with missing external node.

### Step 4: StorypoleWorkspaceStore Unified SQLite Aggregator
* Implement `StorypoleWorkspaceStore` managing child stores.
* Replay child ledgers into a unified workspace SQLite index with `repo` column indexing.
* *Verification Gate:* Querying `storypole list` at workspace level returns tasks aggregated from both root and member stores.

### Step 5: CLI & MCP Routing & Ergonomics
* Update `storypole add`, `storypole defer`, and `storypole update` to route writes by `--repo` or active working directory.
* Add interactive prompt when creating un-scoped task at workspace root in interactive TTY mode.
* *Verification Gate:* CLI integration tests for `storypole add --repo benchwork` writing to `benchwork/.storypole/ledger.jsonl`.

### Step 6: Automated Migration Command (`storypole workspace distribute`)
* Implement `storypole workspace distribute` command:
  1. Partitions existing tasks in root ledger by `scope.repo`.
  2. Scaffolds `.storypole/config.toml` in target repos with appropriate prefixes.
  3. Re-keys tasks with new prefixes while preserving UUIDs and recording legacy aliases.
  4. Moves corresponding `plans/<KEY>.md` design markdown files.
* *Verification Gate:* Run `storypole workspace distribute --dry-run` and live migration; verify `cargo test -p storypole-core` and `storypole list`.
