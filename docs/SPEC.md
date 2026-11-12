# Behavioral Specifications & Invariants — `storypole`

> **Crates:** `storypole`, `storypole-core`, `storypole-cli`, `storypole-mcp`  
> **Storage Engine:** Append-Only JSONL Event Journal + Ephemeral SQLite Projection  
> **Schema Version:** `1.0`

---

## 1. Core Behavioral Invariants

### Invariant 1: Append-Only Immutable Event Sourcing
* `.storypole/ledger.jsonl` (or `.gaugeblock/storypole/ledger.jsonl`) is the sole ground truth.
* Events are append-only; historical lines are never mutated or deleted in place.
* The SQLite database is a disposable derived projection. Deleting `index.sqlite` and replaying the ledger rebuilds identical state deterministically.

### Invariant 2: Topological DAG Acyclicity
* The dependency graph composed of `EdgeKind::BlockedBy` edges must remain a Directed Acyclic Graph (DAG) at all times.
* Any mutation (`link`, `split`, `add`) that would introduce a cycle is refused with `StorypoleError::DependencyCycleDetected`.

### Invariant 3: Branch-Isolated Time-Bounded Leases
* A task can only have one active `AgentClaim` lease at any given time.
* Every claim lease records the claiming `agent_id`, `git_branch`, and `base_commit_sha`.
* An expired lease (`lease_expires_at_epoch_ms < current_epoch_ms`) automatically frees the task for other agents to claim.

---

### Invariant 4: Bidirectional Link Summarisation in Context Briefings
* `TaskContextBriefing.related_tasks_summary` MUST summarise every task linked to the briefing subject by `RelatedTo`, `BlockedBy` or `ParentEpic`, **in either direction**.
* The outgoing pass covers dependencies and the task's own parent epic; the inbound pass covers an epic's children, the tasks a blocker is holding back, and siblings that recorded the link on their own side.
* Mutual links collapse to a single entry, and the wire shape stays `(ticket_key, title, status)` so consumers such as the benchwork inspector render invariantly.
* Resolved decisions inherit across the same link set, so a child briefing also carries its parent epic's resolved decisions.

### Invariant 5: A Receipt Names What It Measured (`GB-316`)
A verification receipt MUST record the repository it measured (`repo`), the configuration key whose
chain ran (`chain_key`), the branch, and the commit range. `repo: None` states that the receipt does not
know which repository it measured — it is a finding, not a default — and a chain that fell back to
`default` is named as `default` rather than left indistinguishable from a match.

Three consequences are load-bearing:

* `verify` refuses to certify a task that names a member's files while carrying no repository scope, and
  the error names the candidate. A receipt about the wrong tree is well-formed and exit-zero, so it
  cannot be spotted afterwards; refusing is visible, certifying the wrong tree is not.
* Scope is resolved once — explicit argument, persisted scope, the task's files, the task's symbols, the
  working directory, then harness — and persisted at claim beside the branch. `verify` reads that
  decision instead of making its own, so the two cannot disagree.
* `storypole show` warns when a receipt's head commit is not reachable from `main` in the repository the
  receipt names. A receipt is written before the merge, so this is asked later; two tickets were once
  certified with their commits stranded on branches `main` never received (`GB-250`, `GB-302`).

The store's location follows the same rule: a member repository's own ledger wins when it holds one, the
suite's canonical ledger is used when the suite declares that member, and a directory holding only an
index is a stub rather than a store (`GB-324`).

---

## 2. System State Machines

### 2.1 Task Node Lifecycle

```mermaid
stateDiagram-v2
    [*] --> Ready: task_created (Dependencies Satisfied)
    [*] --> Blocked: task_created (Has Unresolved Dependencies)
    Ready --> InProgress: task_claimed (Lease Acquired)
    InProgress --> Ready: handoff / claim_expired
    InProgress --> Blocked: decision_requested
    Blocked --> InProgress: decision_resolved (Lease Valid)
    Blocked --> Ready: dependencies_resolved / decision_resolved
    InProgress --> Verified: task_verified (Receipt Recorded)
    Ready --> Deferred: task_deferred
    Verified --> Archived: task_archived
    Archived --> Verified: task_unarchived
```

### 2.2 Agent Claim Lease Lifecycle

```mermaid
stateDiagram-v2
    [*] --> Unclaimed: Task Ready
    Unclaimed --> Leased: claim (agent_id, branch, base_sha, TTL)
    Leased --> Leased: heartbeat (TTL Extended)
    Leased --> Expired: TTL Elapsed without Heartbeat
    Expired --> Unclaimed: Evicted on Next Query
    Leased --> HandoffYielded: handoff (Checkpoint Saved)
    HandoffYielded --> Unclaimed: Lease Released
    Leased --> VerifiedReleased: verify (VerificationReceipt Recorded)
    VerifiedReleased --> [*]: Task Verified
```

---

## 3. Formal Type Contracts

### Boundary Models, Wire Types & Enums

- **`args::AddArgs`** (`struct`): Command-line options for adding a new task to the backlog.
- **`decision::AgentClaim`** (`struct`): Active agent lease containing agent identity, branch name, and expiration timestamp.
- **`args::AskDecisionArgs`** (`struct`): Command-line options for recording an architectural decision requirement.
- **`args::Cli`** (`struct`): Root command-line parser definition for `storypole-cli`.
- **`args::Commands`** (`enum`): Top-level subcommand variants for the CLI.
- **`mutations::CreateTaskParams`** (`struct`): Domain parameters for instantiating a new task node.
- **`args::DecideArgs`** (`struct`): Command-line options for resolving an architectural decision.
- **`decision::DecisionStatus`** (`enum`): Resolution status of an architectural decision (`Pending`, `Resolved`, `Superseded`).
- **`args::DeferArgs`** (`struct`): Command-line options for quickly deferring out-of-scope defects.
- **`discovery::DiscoveredMember`** (`struct`): Discovered member repository within a multi-repo workspace.
- **`task::EdgeKind`** (`enum`): Directed relationship edge between tasks (`BlockedBy`, `RelatedTo`, `ParentEpic`).
- **`args::HandoffArgs`** (`struct`): Command-line options for executing an agent task handoff.
- **`decision::HandoffPacket`** (`struct`): Structured continuity checkpoint recorded during agent handoffs.
- **`args::InitArgs`** (`struct`): Command-line options for initializing a `.storypole` repository.
- **`events::LedgerEvent`** (`enum`): Master append-only serialization event variant written to `ledger.jsonl`.
- **`args::ListArgs`** (`struct`): Filter and formatting parameters for task listing queries.
- **`args::NextArgs`** (`struct`): Parameters for querying the next topologically unblocked ready task.
- **`plan_args::PlanArgs`** (`struct`): Command-line options for attached plan inspection and manipulation.
- **`plan_args::PlanCommands`** (`enum`): Subcommands for managing attached execution plans.
- **`plan::PlanStep`** (`struct`): Discrete actionable step within a `TaskPlan`.
- **`refresh::RefreshReport`** (`struct`): What a projection pass examined, found stale, rewrote and could not read (`GB-349`).
- **`args::SplitArgs`** (`struct`): Command-line options for splitting an oversized task into subtasks.
- **`config::StorypoleConfig`** (`struct`): Configuration options mapped from `.storypole/config.toml` or `.gaugeblock/config.toml`.
- **`error::StorypoleError`** (`enum`): Strongly typed domain error enum for all `storypole-core` operations.
- **`plan::TaskPlan`** (`struct`): Attached execution blueprint containing markdown design, requirements, and steps.
- **`task::TaskStatus`** (`enum`): Lifecycle status of a task (`Ready`, `InProgress`, `Blocked`, `Verified`, `Deferred`, `Archived`).
- **`decision::VerificationReceipt`** (`struct`): Cryptographic verification record certifying task completion with exit codes and commit SHAs.
- **`config::WorkspaceConfig`** (`struct`): Multi-repository workspace configuration mapping child stores.
- **`lib::StorypoleHandler`** (`struct`): The server's tool surface, owning the store behind a `RefCell` because dispatch needs `&mut` and the handler trait hands out `&self`; implements `McpHandler` so the loop and wire rules come from `gaugeblock-mcp`.
