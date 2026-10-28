# Product Vision & Specifications — `storypole`

> **Domain:** Git-Native Semantic Task DAG & Append-Only Ledger  
> **Ecosystem:** `gaugeblock` Neurosymbolic Meta-Harness  
> **License:** Apache-2.0

---

## 1. Problem Statement & Market Context

As software engineering transitions from human-only workflows to hybrid human-agent and multi-agent systems, traditional issue trackers (Jira, Linear, GitHub Issues) fail to address agent execution realities:

1. **Context Fragmentation:** External web trackers live outside the git working tree. When an agent checks out a branch or switches conversations, it loses connection to task criteria, design plans, and dependency states.
2. **Multi-Agent Collisions:** When multiple autonomous agents run in parallel, they lack branch-aware locking. Two agents pick up identical tickets, touch identical files, and generate massive git merge conflicts.
3. **Inline Scope Creep:** When an LLM discovers an edge case or missing AST signature mid-task, it attempts to fix it immediately, bloating git diffs and breaking the main task's invariants.
4. **Unverified Claims & Hallucinated Progress:** Agents claim tasks are completed without verifiable evidence (compiler output, test exit codes, commit SHAs), requiring human engineers to manually audit every claim.

`storypole` solves this by placing an immutable, append-only task DAG directly inside the git repository.

---

## 2. Target Personas & User Journeys

### Persona A: Autonomous AI Coding Agent
- **Needs:** Zero-latency task queries; branch-isolated claim leases; `<5ms` defect deferral without breaking flow; structured plan attachments.
- **Workflow:** Calls `storypole_next` to lease the highest-priority unblocked task, `storypole_set_plan` to persist its design, `storypole_defer` to record discovered tech debt, and `storypole_verify` to submit cryptographic completion receipts.

### Persona B: Systems Software Architect
- **Needs:** Formal architectural decision gates; topological dependency visibility; multi-repo epic coordination.
- **Workflow:** Uses `storypole ask-decision` to enforce human-in-the-loop review on structural choices; inspects DAG topologies via `storypole graph --format mermaid`.

### Persona C: Release & Delivery Engineer
- **Needs:** Auditable verification trails linking git commits to verified requirements.
- **Workflow:** Audits `VerificationReceipt` records in `.storypole/ledger.jsonl` before cutting version tags or triggering automated deployment pipelines.

---

## 3. Jobs to Be Done (JTBD)

| When I... | I want to... | So that I can... |
|---|---|---|
| Launch an autonomous agent | Assign it an unblocked priority task with an exclusive claim lease | Prevent it from colliding with other agents or picking up blocked downstream dependencies. |
| Encounter an unrelated defect during code generation | Record a structured task in `<5ms` without leaving my active file | Preserve technical debt tracking without allowing the agent to drift into out-of-scope refactoring. |
| Require human review on an architectural trade-off | Block task execution on an explicit decision gate (`ask-decision`) | Prevent the agent from making unvetted architectural changes without human sign-off. |
| Complete a multi-step task | Attach compiler exit codes, diagnostic logs, and commit SHAs (`verify`) | Provide cryptographic proof of correctness for continuous integration and release gates. |

---

## 4. Key Metrics & Success Criteria

* **Query & Defer Latency:** `<5ms` for `storypole defer` and `storypole next` operations.
* **Agent Collision Rate:** `0%` task claim collisions across parallel agent subshells.
* **Storage Invariance:** Zero git merge conflict corruption via append-only JSON Lines event sourcing.
* **Verification Fidelity:** `100%` of closed tasks accompanied by an immutable `VerificationReceipt`.
