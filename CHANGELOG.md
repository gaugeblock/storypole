# Changelog

All notable changes to `storypole` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
