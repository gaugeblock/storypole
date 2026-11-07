# Architectural Design & Decision Records — `storypole`

> **Crates:** `storypole`, `storypole-core`, `storypole-cli`, `storypole-mcp`  
> **Status:** Active Architectural Decision Records (ADRs)

---

## Architectural Decision Records (ADRs)

### ADR-001: Append-Only JSONL Event Sourcing with Ephemeral SQLite Projections
- **Status:** Accepted
- **Context:** Storing backlog state in pure SQLite or raw markdown issues creates severe git merge conflict hazards across concurrent branches, while raw JSON files are prone to accidental line overwrites.
- **Decision:** Establish `.storypole/ledger.jsonl` as the append-only source of truth. Every task creation, status transition, lease claim, and verification receipt appends a discrete JSON line. Maintain `.storypole/index.sqlite` strictly as a git-ignored, ephemeral projection that is deterministically rebuilt on startup or file modification.
- **Consequences:** Merging git branches never corrupts binary database files, and diff inspections remain completely transparent in pull requests.

### ADR-002: Topological DAG Task Scheduling & Cycle Prevention
- **Status:** Accepted
- **Context:** Complex software engineering projects require strict ordering constraints (e.g. core interfaces must compile before dependent CLI handlers are authored). Linear backlogs lack dependency semantics.
- **Decision:** Model task dependencies as directed graphs with typed edges (`BlockedBy`, `RelatedTo`, `ParentEpic`). Execute topological sort traversal to identify unblocked `Ready` tasks, and immediately reject any edge insertion that would create a cyclic dependency.
- **Consequences:** Agents always receive the highest-priority truly actionable task with zero risk of deadlocks.

### ADR-003: Branch-Isolated Time-Bounded Agent Leases & Base Commit Tracking
- **Status:** Accepted
- **Context:** Multiple autonomous agents or parallel coding subshells operating on the same repository risk claiming the same task, creating conflicting edits and merge collisions.
- **Decision:** Implement atomic claim leases (`storypole claim <KEY> <AGENT_ID> [--branch <BRANCH>]`) with configurable time-to-live (TTL). Record the claiming agent identity, git branch name, and base commit SHA in the lease. Require periodic heartbeats to maintain active leases.
- **Consequences:** Zero agent collisions, explicit work-in-progress visibility, and automated recovery if an agent crashes or abandons a lease.

### ADR-004: Dual-Store Hierarchy (Child `.storypole/` vs Workspace `.gaugeblock/storypole/`)
- **Status:** Accepted
- **Context:** In a multi-repo meta-harness, member repositories must function as standalone open-source projects while allowing cross-cutting ecosystem epics at the root level.
- **Decision:** Support dual-store discovery: member checkouts maintain local `.storypole/` directories for sub-crate work, while the workspace root maintains `.gaugeblock/storypole/` for ecosystem milestones. Provide `storypole workspace distribute` to route tasks cleanly across boundaries.
- **Consequences:** Full standalone Unix utility for single repositories, seamless aggregation for complex meta-harnesses.

### ADR-005: Zero-Inline Scope Creep & Asynchronous Defect Deferral
- **Status:** Accepted
- **Context:** Autonomous coding agents frequently lose focus when encountering unexpected bugs or edge cases outside their immediate assignment, attempting to fix them inline and bloating diffs.
- **Decision:** Mandate non-negotiable deferral via `storypole defer --kind <bug|tech-debt|enhancement> --symbol <crate::path> <title>`. Optimize the command path for sub-5ms execution to make recording discoveries frictionless.
- **Consequences:** Agents remain locked on primary objectives while capturing 100% of discovered technical debt for future backlog triage.
