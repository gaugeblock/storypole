# `storypole` — Git-Native Semantic Task DAG & Append-Only Ledger

[![Version](https://img.shields.io/badge/version-0.1.0--beta.3-blue.svg)](https://gaugeblock.dev)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](LICENSE)
[![gaugeblock](https://img.shields.io/badge/suite-gaugeblock-orange.svg)](https://github.com/gaugeblock)
[![Documentation](https://img.shields.io/badge/docs-9--doc%20topology-green.svg)](ARCHITECTURE.md)

Part of the **[`gaugeblock`](https://gaugeblock.dev)** neurosymbolic code synthesis ecosystem.

> In traditional carpentry, a **story pole** (or story rod) is a single, precision-marked wooden rule containing all the critical vertical dimensions of a structure—ceiling heights, window openings, and dado joints. It serves as the immutable master reference across the jobsite, preventing measuring tape discrepancies. In the `gaugeblock` suite, `storypole` is the single source of truth for autonomous agent workflows: a Git-tracked, append-only task DAG and MCP memory server ensuring deterministic task ordering, claim leases, architectural decision records, and multi-commit verification receipts.

---

## 1. Architecture Overview

```text
┌────────────────────────────────────────────────────────┐
│               Git Repository Working Tree              │
│  .storypole/ledger.jsonl  •  .gaugeblock/storypole/    │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│ storypole Engine (SQLite Materialized View & Traversal)│
│ - Append-only JSONL event journal                      │
│ - Zero-latency in-memory / local SQLite projection     │
│ - Multi-repository workspace rollup & store routing    │
│ - Topological DAG scheduler (Ready / Blocked / Leased) │
└───────────────┬────────────────────────┬───────────────┘
                │                        │
                ▼                        ▼
┌───────────────────────────┐ ┌──────────────────────────┐
│ CLI (`storypole`)         │ │ MCP Stdio Server         │
│ - storypole next / claim  │ │ - storypole_claim        │
│ - storypole defer         │ │ - storypole_next         │
│ - storypole verify        │ │ - storypole_verify       │
│ - storypole plan set      │ │ - storypole_ask_decision │
└───────────────────────────┘ └──────────────────────────┘
```

---

## 2. Core Capabilities

* **Append-Only Git-Native Ledger:** All task events, status transitions, leases, and receipts are recorded as JSON Lines in `.storypole/ledger.jsonl`, committed directly alongside code changes.
* **Topological DAG Scheduling:** Automatically computes task dependencies, identifying unblocked `Ready` tasks while respecting `BlockedBy`, `RelatedTo`, and `ParentEpic` relationships.
* **Branch-Isolated Agent Leases:** Prevents collision between autonomous agents via time-bounded leases (`storypole claim`), recording active branch names and base commit SHAs.
* **Architectural Decision Gates:** Blocks tasks requiring human design approval via `ask-decision` until explicitly resolved via `decide`.
* **Zero Inline Scope Creep (`defer`):** Allows agents encountering out-of-scope defects to instantly record tech debt, bugs, or refactorings without breaking their primary execution loop.
* **Specification & Plan Persistence:** Direct attachment of markdown design plans (`.storypole/plans/<KEY>.md`) with discrete verification gates per step.
* **Multi-Repository Rollup:** Seamless traversal from child repositories to workspace meta-harness roots with distributed store synchronization.

---

## 3. Workspace Crate Layout

* **`storypole`**: Facade library re-exporting `storypole-core`.
* **`crates/storypole-core`**: Pure domain types, SQLite projection engine, DAG traverser, ledger reader/writer, and multi-repo store routers.
* **`crates/storypole-cli`**: Clap binary entrypoint (`storypole`).
* **`crates/storypole-mcp`**: JSON-RPC 2.0 Model Context Protocol stdio and SSE daemon.

---

## 4. Installation & CLI Quickstart

```bash
# Install globally via cargo
cargo install --path crates/storypole-cli

# Initialize a storypole ledger in the current repository
storypole init

# Inspect ready and in-progress tasks
storypole list
storypole list --status InProgress

# Retrieve the next unblocked priority task
storypole next

# Claim a task with branch isolation
storypole claim GB-12 agent-antigravity --branch feat/gb-12-auth

# Defer out-of-scope defects without losing context
storypole defer --kind bug --symbol "crate::parser" --title "Handle unexpected EOF in tokenizer"

# Record an architectural decision requirement
storypole ask-decision GB-12 "Use Redis or in-memory LRU for session cache?" --option "Redis" --option "LRU"

# Resolve decision
storypole decide GB-12 <DECISION_ID> --choose "Redis" --rationale "Multi-instance clustering requirement"

# Attach design blueprint and execution plan
storypole plan set GB-12 --file .storypole/plans/GB-12.md --summary "Implement JWT authentication"

# Verify task completion with compiler checks
storypole verify GB-12 agent-antigravity --summary "Passed Tier 3 workspace checks"
```

---

## 5. Model Context Protocol (MCP) Reference

When operating inside AI agents (Antigravity, Claude Code, Cline), `storypole` registers native tools via stdio:

| Tool | Parameters | Description |
|---|---|---|
| `storypole_next` | `agent_id?` | Peek or claim the highest-priority topological task ready for work. |
| `storypole_show` | `key` | Retrieve full task metadata, criteria, plan, and decision state. |
| `storypole_add` | `title, desc, kind, priority, repo` | Add a new task node to the backlog DAG. |
| `storypole_update` | `key, title?, desc?, kind?, priority?` | Mutate an existing task node's parameters. |
| `storypole_link` | `source, target, kind` | Connect tasks via `blocked_by`, `related_to`, or `parent_epic`. |
| `storypole_defer` | `title, kind, symbol, desc?` | Defer out-of-scope tech debt or bugs discovered during execution. |
| `storypole_claim` | `key, agent_id, branch?` | Lease a task with branch isolation and base commit SHA tracking. |
| `storypole_heartbeat` | `key, agent_id` | Refresh active lease TTL during long-running tasks. |
| `storypole_handoff` | `key, agent_id, summary` | Release task lease back to DAG with handoff notes. |
| `storypole_verify` | `key, agent_id, summary?` | Mark task verified, recording commit SHAs and exit codes. |
| `storypole_ask_decision` | `key, question, options` | Block task on pending architectural decision. |
| `storypole_decide` | `key, decision_id, choice, rationale` | Resolve pending decision and unblock execution. |
| `storypole_set_plan` | `key, plan_markdown` | Attach comprehensive execution plan with step checklist. |

---

## 6. Repository Documentation

`storypole` adheres strictly to the canonical **9-Document Architecture**:
* **[`CONTRIBUTING.md`](./CONTRIBUTING.md):** Local toolchain setup, developer prerequisites, and test execution.
* **[`ARCHITECTURE.md`](./ARCHITECTURE.md):** Internal sub-crate topologies, SQLite projection, and store synchronization.
* **[`SPEC.md`](./SPEC.md):** Formal event schemas, ledger serialization contracts, and state transition rules.
* **[`AGENTS.md`](./AGENTS.md):** Non-negotiable engineering directives, file-size ceilings (<=250 LOC), and zero-unwrap rules.
* **[`CHANGELOG.md`](./CHANGELOG.md):** Chronological historical release entries and unreleased changes.

---

## 7. Verification & Development Directives

* **Hard 250-Line Limit:** Every `.rs` file must remain strictly under 250 lines of code.
* **Tiered Verification:**
  * Tier 1: `cargo check -p storypole-core` (<2s).
  * Tier 2: `cargo fmt --check -p storypole-core && cargo clippy -p storypole-core && cargo test -p storypole-core`.
  * Tier 3: `chalkline docs check -d .` before task verification.
* **Zero Unwrap:** Production paths in `storypole-core` forbid `.unwrap()` and `.expect()`.

---

## 8. License

Licensed under the [Apache License, Version 2.0](LICENSE).
