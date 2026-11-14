# Changelog

All notable changes to `storypole` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [Unreleased]

### Documentation
- `storypole-core`: ARCHITECTURE §2.1 now states which state follows the branch and which is shared, because the two resolve differently on purpose and confusing them is how a worktree appears to lose its backlog: tickets and events are tracked source, claim leases and ticket numbers live in the git common dir so two worktrees cannot claim or mint the same thing, and `index.sqlite` is a per-worktree cache of tracked source (SP-45).
- `storypole-cli`: the README now states that archiving preserves the status a task earned, so `status = triage` with `archived_at_epoch_ms` set means *archived*, not open, and gives the `list --json --include-archived` query that counts open work from rows rather than from ticket files. Two wrong counts in this suite came from filtering on status alone - a sweep that reported 8 open rows where the store held 2, and a board that reported 148 where replay said 78. Two tests pin the row shape the query depends on. [GB-360]

### Fixed
- `storypole-cli`: `migrate` now reports both of its effects. The dry run names the layout transition (`Ledger -> Records`), the number of records applying would write, and where the ledger will be archived, in the table and in `--json` (`layout`, `target`, `records`); `--help` describes the layout change as well as the repository split; and a store where every ticket stays at the root no longer closes with "nothing was written and the ledger is untouched", which read as nothing to do for exactly the repository that most needed converting. A single-repository store previously saw `moving 0` while applying wrote every record and archived its ledger. `--dry-run` remains read-only. [GB-357]
- `storypole-cli`: `workspace distribute --help` no longer describes itself as moving tasks "from root ledger into their respective repository ledgers", wording that predates the Records layout and made a second verb read like `migrate`. [GB-357]
- `storypole-cli`: `add` now splits `--files`, `--symbols` and `--labels` on commas exactly as `update` does, and both verbs accept both spellings (`--files`/`--file`, `--symbols`/`--symbol`, `--labels`/`--label`). A comma-joined value used to be stored as one scope entry, so `--files a.rs,b.rs` produced a single path that exists nowhere while `update` produced two - the damage landed in the record and survived until somebody read it, which is how a sibling project filed a ticket naming two files as `["AGENTS.md,.gaugeblock/config.toml"]`. Criteria stay repeatable-only and are not comma-split: one criterion containing a comma is one criterion. [GB-359]
- `storypole-core`: `event_timestamp` now reads `HandoffRecorded` from the packet's own `timestamp_ms` instead of letting it fall through to the zero arm of the timestamp table, so a handoff no longer sorts ahead of every real event. Replay by `(timestamp, id)` and replay from a legacy ledger's file order now agree, which is what `migrate` checks before it archives a ledger. Before this, any store holding a handed-off ticket that was never subsequently verified could not be migrated: `migrate` aborted with a divergence error and the written record reported the stale status (SP-54).
- `storypole-core`: `claim` now creates and checks out the lease branch in the repository that owns the task (`scope.repo`) instead of the store root, so claiming a member task can no longer switch a harness checkout onto a feature branch; harness-level tasks record the branch without touching any checkout, and verification receipts read branch/SHA from the task's own repository (GB-137).

### Changed
- `storypole-core`: index connections open with `journal_mode = WAL`, `synchronous = NORMAL`, `temp_store = MEMORY`, a 64 MiB page cache and a 5 s `busy_timeout`, removing the double `fsync()` per transaction and letting concurrent CLI/MCP/daemon readers proceed without blocking the writer. [GB-132]

### Fixed
- `storypole-core`: context briefings now summarise `ParentEpic` links and inbound edges, so epics list their children and freshly linked tasks appear in `storypole show` and the benchwork inspector. Mutual links are deduplicated. [GB-136]

## [v0.1.0-beta.5] - 2026-05-09

### Maintenance
- Elevated the storypole 9-document architecture to gold standard. [GB-109]

## [v0.1.0-beta.4] - 2026-05-09

### Added
- Established the 9-doc architecture for the storypole documentation set.

### Fixed
- No changes.

### Changed
- Standardized the README to align with the new 9-doc architecture.

### Maintenance
- Updated documentation conventions across the storypole workspace.

### Documentation
- Standardize README to 9-section template and establish AST-grounded 9-document architecture

## [v0.1.0-beta.3] - 2026-05-09

### Added
- Add multi-repository workspace discovery, upward traversal, and distributed stores for first-class multi-repo support in `storypole` [GB-57].

### Fixed
- Use an in-memory SQLite schema instead of a temporary on-disk file for store initialization in `storypole` [GB-57].

### Changed
- Migrate scoped tasks and design plans to the updated workspace model in `storypole`.

### Maintenance
- Update release history with alpha and beta milestones in `storypole`.

## [0.1.0-beta.2] - 2026-10-24

### Added
- `storypole-core`: support unified `.gaugeblock/` configuration directory and state consolidation (GB-36).
- `storypole-core`: task archiving and unarchiving lifecycle support (`storypole archive`, `storypole unarchive`) (GB-34).
- `storypole-core`: `TaskPlan` specification persistence and unabridged design markdown in `.storypole/plans/<KEY>.md` (GB-28).
- `storypole-core`: branch isolation tracking and base commit recording in `AgentClaim` leases (GB-28).
- `storypole-cli`: `storypole update` command for mutating task nodes, descriptions, kinds, priorities, and scopes (GB-32).
- `storypole-cli`: integrated `miter` compiler verification and target path resolution during task verification (GB-26).
- `storypole-core`: short commit ref resolution in `VerificationReceipt` (GB-25).

### Fixed
- `storypole-cli`: parse tech-debt hyphenated kind in CLI arguments (GB-35).

## [0.1.0-beta.1] - 2026-10-21

### Added
- Git-native append-only JSONL event stream (`.storypole/ledger.jsonl`) with git merge conflict marker resilience.
- Ephemeral SQLite index (`.storypole/index.sqlite`) with automatic synchronization and multi-dimensional querying.
- Topological task DAG engine with cycle detection on `BlockedBy` and `ParentEpic` edges.
- Chained decision lifecycle with `Pending`, `Resolved`, and cascading `Superseded` states.
- Agent continuity protocol with TTL leases, heartbeats, and structured handoff packets.
- Zero-interruption issue deferral (`storypole defer`) in <5ms with provenance tracking.
- Stdio JSON-RPC 2.0 Model Context Protocol (MCP) server exposing 14 tools for AI agent pair programming.
- CLI binary with human-readable tables/trees and machine-parseable `--json` output.
- Hard 250-line source file limit enforcement across all modules.

## [0.0.1-alpha.1] - 2026-10-21

### Added
- Initial namespace reservation on crates.io and GitHub repository scaffold.
