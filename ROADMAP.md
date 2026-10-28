# Product Roadmap & Strategic Horizons — `storypole`

> **Active Suite Lifecycle:** `0.1.0-beta.X`  
> **Target:** Git-Native Semantic Task DAG & Multi-Agent Memory Store

---

## 1. Active Milestone: `v0.1.0-beta.4` (Current Focus)

- [x] **Append-Only JSONL Event Stream:** Immutable `.storypole/ledger.jsonl` event sourcing.
- [x] **Topological DAG Scheduler:** Acyclic dependency graph resolution (`BlockedBy`, `RelatedTo`, `ParentEpic`).
- [x] **Branch-Isolated Agent Leases:** Time-bounded claim leases with branch name and base commit SHA tracking.
- [x] **Architectural Decision Gates:** `ask-decision` and `decide` commands blocking task execution until human sign-off.
- [x] **Zero-Friction Deferral:** Sub-5ms `storypole defer` for immediate out-of-scope defect capture.
- [x] **Model Context Protocol (MCP):** JSON-RPC 2.0 stdio server exposing 13 core agent tools.
- [x] **9-Document Architecture Compliance:** Certified clean under `chalkline docs check`.

---

## 2. Horizon 1: Full-Text Search, TUI & Issue Sync (`v0.2.0`)

- [ ] **SQLite FTS5 Full-Text Search:** Sub-millisecond natural-language search across task descriptions and criteria.
- [ ] **Interactive Terminal Kanban Board (TUI):** `storypole tui` interactive terminal interface built with `ratatui` for human backlog navigation.
- [ ] **Bidirectional Web Tracker Sync:** Optional background daemon synchronizing tasks with GitHub Issues and Linear.
- [ ] **Context Window Slicing:** Automated pruning of old completed receipts when generating prompt context envelopes.

---

## 3. Horizon 2: Swarm Coordination & CRDT Merge Resolution (`v0.3.0`)

- [ ] **Multi-Agent Swarm Coordinator:** Local WebSocket daemon coordinating claim leases across 10+ concurrent agents.
- [ ] **CRDT Merge Resolver:** Automated conflict resolution for append-only ledgers across divergent git topic branches.
- [ ] **Continuous Defect Clustering:** Machine learning clustering of deferred items into coherent refactoring epics.

---

## 4. Horizon 3: Enterprise Compliance & Memory Fabric (`v1.0.0`)

- [ ] **Cryptographic Audit Exporter:** Export cryptographically verifiable compliance reports linking requirements to verified commit SHAs.
- [ ] **Zero-Copy Memory-Mapped IPC:** Shared-memory ring buffer connecting `storypole` task state directly to `joiner` and `benchwork`.
- [ ] **Long-Term Wire Stability:** Freeze JSONL event serialization schema for multi-year backward compatibility.
