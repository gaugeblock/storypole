# Changelog

All notable changes to `storypole` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
