# `storypole` — Git-Native Semantic Task DAG & Append-Only Ledger

[![Version](https://img.shields.io/badge/version-0.1.0--beta.4-blue.svg)](https://gaugeblock.dev)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](LICENSE)
[![gaugeblock](https://img.shields.io/badge/suite-gaugeblock-orange.svg)](https://github.com/gaugeblock)
[![Documentation](https://img.shields.io/badge/docs-9--doc%20topology-green.svg)](docs/ARCHITECTURE.md)

Part of the **[`gaugeblock`](https://gaugeblock.dev)** neurosymbolic code synthesis ecosystem.

> In traditional carpentry, a **story pole** (or story rod) is a single, precision-marked wooden rule containing all the critical vertical dimensions of a structure—ceiling heights, window openings, and dado joints. It serves as the immutable master reference across the jobsite, preventing measuring tape discrepancies. In the `gaugeblock` suite, `storypole` is the single source of truth for autonomous agent workflows: a Git-tracked, append-only task DAG and MCP memory server ensuring deterministic task ordering, claim leases, architectural decision records, and multi-commit verification receipts.

---

## Why `storypole`?

Autonomous coding agents (Claude Code, Cursor, Cline, Windsurf) excel at localized editing, but collapse when coordinating multi-step software engineering across conversations and branches.

| Without `storypole` | With `storypole` |
|---|---|
| **Context Window Amnesia:** When an agent's context window truncates or a session restarts, all task progress, pending sub-tasks, and architectural rationales evaporate. | **Git-Native Append-Only Journal:** Every event is committed to `.storypole/ledger.jsonl`. Progress survives context resets, session reboots, and branch checkouts permanently. |
| **Multi-Agent Collision:** Two agents or subshells work on identical tasks simultaneously, overwriting files, causing git merge conflicts, and duplicating effort. | **Branch-Isolated Claim Leases:** `storypole claim` establishes time-bounded leases recording the agent ID, active branch, and base commit SHA with heartbeat renewals. |
| **Inline Scope Creep:** When an agent encounters an unrelated bug or edge case mid-task, it attempts to fix it inline—diverting focus, bloating diffs, and breaking builds. | **Zero-Friction Deferral:** `storypole defer` records out-of-scope defects or refactorings in `<5ms` to the backlog DAG so the agent stays locked on its active task. |
| **Unrecorded Architectural Drift:** Agents make ad-hoc structural decisions in ephemeral chat messages that human reviewers never see or understand. | **Architectural Decision Gates:** `storypole ask-decision` halts task execution until human engineers approve design choices via `storypole decide`, preserving permanent ADR audit trails. |
| **Unverified Merges:** Tasks are marked "done" without evidence, leading to broken CI pipelines and regressions. | **Cryptographic Verification Receipts:** `storypole verify` attaches `VerificationReceipt` records capturing the verifier, exit code, and commit SHAs to certify completion. |

---

## 1. System Architecture

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

## 2. CLI Commands Reference

`storypole` provides a complete command suite for terminal workflows, automated scripting, and agent tool execution. Pass `--json` to any command for structured JSON output.

| Command | Description | MCP Tool |
|---|---|---|
| `init` | Initialize a new `.storypole` ledger repository in current directory. | — |
| `add` | Add a new backlog task node with title, priority, and criteria. | `storypole_add` |
| `list` | List backlog tasks matching status, priority, or kind filters. | — |
| `show <KEY>` | Show full context briefing, criteria, plan, and decisions for a task. | `storypole_show` |
| `next` | Find next topologically unblocked ready task. | `storypole_next` |
| `claim <KEY> <AGENT>` | Acquire active time-bounded agent lease with branch tracking. | `storypole_claim` |
| `heartbeat <KEY> <AGENT>`| Renew active lease TTL for long-running execution. | `storypole_heartbeat` |
| `handoff <KEY> <AGENT>` | Record structured continuity packet and release claim lease. | `storypole_handoff` |
| `defer` | Defer out-of-scope defects, bugs, or debt in `<5ms`. | `storypole_defer` |
| `link <SRC> <TGT>` | Connect tasks with directed DAG edges (`blocked_by`, `related_to`, `parent_epic`). | `storypole_link` |
| `split <KEY>` | Split an oversized task into an Epic parent and child subtasks. | `storypole_split` |
| `ask-decision <KEY>` | Raise an architectural decision requirement, blocking task. | `storypole_ask_decision` |
| `decide <KEY> <ID>` | Resolve pending decision with chosen option and rationale. | `storypole_decide` |
| `supersede-decision <KEY>`| Supersede a previously resolved decision with updated rationale. | `storypole_supersede_decision` |
| `verify <KEY>` | Certify completion and record a `VerificationReceipt`. | `storypole_verify` |
| `update <KEY>` | Mutate an existing task node's title, description, or priority. | `storypole_update` |
| `plan set <KEY>` | Attach execution plan blueprint (`.storypole/plans/<KEY>.md`). | `storypole_set_plan` |
| `graph` | Display ASCII or Mermaid dependency DAG visualization. | — |
| `archive <KEY>` | Archive a completed or closed task out of active backlog views. | — |
| `unarchive <KEY>` | Restore an archived task back to active backlog views. | — |
| `park <KEY> [--reason]` | Park a task: it stays in the backlog and leaves the ready queue. Refuses a verified task, a held claim, or an archived one, so parking cannot hide finished work. | — |
| `unpark <KEY>` | Return a parked task to the backlog — as `blocked`, not `ready`, when a task it depends on is unverified. | — |
| `reopen <KEY> --reason` | Reopen a verified task, keeping the receipt that certified it so the record still says what was verified and when. | — |
| `remint` | Report, or with `--apply` perform, the key remint a store needs to mint under its own prefix, rewriting every reference in the same pass. | — |
| `relink` | Report, or with `--apply` perform, the cross-store repair a per-store remint cannot do: rewrite citations of keys that moved elsewhere and link harness tickets to the tickets carrying the work. | — |
| `workspace` | Manage multi-repository workspace members and store routing. Subcommands: `list`, `init`, `distribute`. | — |
| `spool migrate` | Rewrite legacy bare-trace outbox lines into canonical `SpoolEntry` envelopes. | — |
| `mcp` | Launch the Model Context Protocol (JSON-RPC 2.0 stdio) server. | — |

---

## 3. Model Context Protocol (MCP) Server

To connect `storypole` to Antigravity, Claude Code, Cursor, or Cline, register the stdio server in your MCP configuration (`~/.gemini/antigravity/mcp_config.json`):

```json
{
  "mcpServers": {
    "storypole": {
      "command": "storypole",
      "args": ["mcp"]
    }
  }
}
```

Every MCP mutation automatically appends to `.storypole/ledger.jsonl` and updates the SQLite projection view instantly without server restarts.

---

## 4. Installation & Quickstart

```bash
# Install globally from source
cargo install --path crates/storypole-cli

# Initialize repository
storypole init --prefix SP

# Query next ready task
storypole next

# Claim task with dedicated branch isolation
storypole claim SP-1 agent-antigravity --branch feat/sp-1-parser

# Record an unexpected defect discovered during coding
storypole defer --kind bug --symbol "crate::tokenizer" --title "Fix unexpected EOF in string scanner"

# Mark task verified and record commit receipts
storypole verify SP-1 agent-antigravity --summary "Completed unit tests and passes clippy"

# Return tasks to the backlog when a claim lease expires (crashed agent, closed laptop)
storypole reap --dry-run
```

### Reporting open work: archived tasks keep their status

Archiving records *that* a task left the backlog; it does not rewrite what it was when it left. A task archived
from triage still carries `status = triage` with `archived_at_epoch_ms` set, and a task archived after
verification still reads `verified`. `storypole list` hides archived rows by default and says how many it hid,
but anything that reads ticket files directly - or filters a listing on status alone - counts closed work as
open. Measured twice: a suite sweep reported 8 triage rows where the store held 2, and a board count reported
148 active tickets where the answer was 78 (`GB-349`, `GB-360`).

Ask the store, and test the archive marker rather than the status. The marker is **omitted** on an open row rather
than set to null, so read it defensively - a missing key and a null both mean open, and a strict reader that
indexes it directly will fail on the rows it most wants:

```bash
# Open work, counted from rows that carry both fields. Never glob tickets/*.json and filter on status.
storypole list --json --include-archived \
  | jq '[.tasks[] | select(.archived_at_epoch_ms == null)] | length'

# Everything archived from triage, which is invisible to the line above by construction
storypole list --json --include-archived \
  | jq '[.tasks[] | select(.archived_at_epoch_ms != null and .status == "triage")] | length'
```

### Ledger integrity and index reconciliation

A distributed ledger set has no compiler, so these two commands make drift observable. Both exit
non-zero on findings, which means they can gate:

```bash
# Structural audit: parse failures, duplicate events, replayed status histogram
storypole ledger-verify

# Compare the unified index against the replay, per status
storypole ledger-reconcile
```

`ledger-verify` reports `1078 events across 11 files: 0 unparsable, 0 duplicate` on a healthy suite.
`ledger-reconcile` ends in `index and ledger agree` when the SQLite projection matches the ledgers,
and names each status group that disagrees otherwise.

A third reader can disagree for a reason neither of those two looks at: each ticket file carries a `ticket`
block that projects its own `events`, and a block written before the projection was kept current can hold a
status the log has moved past (`GB-349`). `refresh-projections` reports those files and rewrites them only
when asked, so a reader that never replays gets the same answer as one that does:

```bash
# Report records whose ticket block disagrees with their events
storypole refresh-projections

# Rewrite them; a second pass then reports every projection in agreement
storypole refresh-projections --apply
```

### Key migration

A member store that inherited another repository's key numbering can be moved onto its own configured prefix. The
command reports first and changes nothing until asked:

```bash
# Report the mapping and what a rewrite would have to repair
storypole remint --dir path/to/repo

# Apply it: keys, edge targets, citations, plan filenames and plan paths in one pass
storypole remint --apply --dir path/to/repo
```

A key is not only an identifier - it is how one ticket points at another - so applying rewrites the references in the
same pass and then verifies against replay rather than against the files it just wrote. That distinction is the whole
point: a ticket file is a projection plus an event log, replay applies `TaskUpdated.description`,
`EdgeAdded.edge` and `PlanUpdated.plan` from the event payload, and a pass that rewrote only the projection would leave
the served state citing keys nothing owns while reporting success. A claim's `branch` is deliberately left alone: it
records where work happened, and renaming it would name a branch that never existed.

`remint` changes what a ticket is called now, never what it was called when it shipped - changelogs and commit messages
are history and are not touched.

A remint records what it moved in `<store>/remints.json`, keyed by the task uuid, because a key is a name and the next
migration needs to resolve a reference to it. That record is what makes the other half possible:

```bash
# Report the references a per-store remint could not see, and the links it would write
storypole relink

# Repair them and write the links
storypole relink --apply
```

A remint works one store at a time, so it cannot see that the harness store still cites `GB-189` after common's
`GB-189` became `CO-7`. `relink` reads every store's record, decides which keys are genuinely dead (a key the harness
still uses is not dead, and a key two stores both claim is reported rather than guessed), rewrites those citations, and
- when the citing ticket is a harness ticket - writes a `related_to` edge to the ticket now carrying the work. The edge
is written by uuid, not by key, so it survives the next rename; and it is appended as an `edge_added` event, because
replay rebuilds the task from the log.

A blueprint is a file named for its ticket, and the ticket carries a pointer to it. When the two disagree - the file sits
in the canonical plans directory while the ticket lives in a member, or the pointer names a file that was never written -
nothing said so until `ledger-verify` began auditing it. The move is mechanical once the pointer is trusted, because the
pointer already names the destination:

```bash
# Report the blueprints that can be placed, and the pointers that cannot be
storypole plan collect

# Move them
storypole plan collect --apply
```

### Training-trace outbox

`storypole verify` appends one record per verified head to
`<canonical .gaugeblock>/spool/outbox.jsonl`, keyed on `(task_key, head_commit_sha)` so re-verifying
the same commit is a no-op. The record is `gaugeblock_contracts::SpoolEntry` - the same type `joiner`
spools and `rabbet` ingests - so one outbox file is readable by every tool in the suite.

Outboxes written before `GB-182` may still hold bare traces with no envelope. `spool migrate`
rewrites those in place, computing the canonical SHA-256 content hash and leaving already-enveloped
lines byte-for-byte untouched; a second run reports zero rewrites.

```bash
# Preview the rewrite without touching the file
storypole spool migrate --dry-run

# Rewrite legacy lines, then re-run to confirm the outbox is canonical
storypole spool migrate
```

---

## 5. Canonical 9-Document Architecture

`storypole` maintains 100% compliance with the canonical 9-Document Architecture:
* **[`README.md`](./README.md):** Public elevator pitch, command reference, and MCP integration.
* **[`CONTRIBUTING.md`](CONTRIBUTING.md):** Contributor onboarding, toolchain setup, and verification gates.
* **[`PRODUCT.md`](docs/PRODUCT.md):** User personas, JTBD, and neurosymbolic task orchestration value.
* **[`ARCHITECTURE.md`](docs/ARCHITECTURE.md):** Internal sub-crate topologies, SQLite projection, and store synchronization.
* **[`SPEC.md`](docs/SPEC.md):** Formal event schemas, ledger serialization contracts, and state transition rules.
* **[`DESIGN.md`](docs/DESIGN.md):** Architectural Decision Records (ADRs) and trade-off analyses.
* **[`ROADMAP.md`](docs/ROADMAP.md):** Strategic milestones, upcoming capabilities, and horizons.
* **[`CHANGELOG.md`](./CHANGELOG.md):** Chronological historical release logs and unreleased entries.
* **[`AGENTS.md`](./AGENTS.md):** Operational directives, 250-line file ceiling, and verification matrix.

---

## 6. License

Licensed under the [Apache License, Version 2.0](LICENSE).
