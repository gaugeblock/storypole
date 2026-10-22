# storypole

> Git-native SQLite and JSONL backlog, task DAG, and MCP memory store for AI coding agents.

Part of the **gaugeblock** neurosymbolic code synthesis ecosystem alongside [`miter`](https://crates.io/crates/miter-cli).

*Status: v0.0.1-alpha.1 namespace reservation — active development in progress.*

---

## Workspace & Development Standards

`storypole` is organized as a Rust 2024 workspace under `crates/`:
* `crates/storypole-core`: Core data structures, task DAG, and append-only ledger.
* `crates/storypole-cli`: Command-line interface and daemon commands.
* `crates/storypole-mcp`: Model Context Protocol (MCP) stdio and SSE server.

All contributions adhere to the `gaugeblock` suite standards:
* **Tiered Verification:** Tier 1 for rapid inner-loop editing, Tier 2 before atomic commits, and Tier 3 (`cargo fmt`, `cargo clippy`, `cargo test`) for full workspace checks.
* **Atomic Conventional Commits:** `<type>(<scope>): <summary>` (e.g. `feat(storypole-core): add ledger event parser`).
* **Semantic Versioning:** Synchronized workspace versioning inheriting `0.0.1-alpha.1` with `vX.Y.Z` release tags.

See root [`CONTRIBUTING.md`](../CONTRIBUTING.md) and [`AGENTS.md`](../AGENTS.md) for full directives.
