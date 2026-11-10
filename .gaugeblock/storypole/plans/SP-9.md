# Task Plan: GB-136 — TaskBriefing Omits Parent-Epic and Incoming Edges

> **Task Key:** `GB-136`  
> **Scope:** `storypole-core`  
> **Related:** `GB-97` (benchwork wire contracts), `GB-121`/`GB-128` (epics that render empty)  
> **Status:** Planned

---

## 1. Problem Statement

`build_task_briefing()` summarises only **outgoing** `RelatedTo` / `BlockedBy` edges:

```rust
if matches!(edge.kind, EdgeKind::RelatedTo | EdgeKind::BlockedBy)
```

Two consequences reach the user through both the `storypole show` CLI and the benchwork
inspector (`InspectorEdges.tsx` → `briefing.related_tasks_summary`):

1. **Epics render empty.** `ParentEpic` edges are stored **on the child** (see
   `ops::graph::split_task`), so an epic such as `GB-121` — which owns five children in the ledger —
   reports `Related Tasks & Edges (0)` and the inspector prints "No linked tasks or dependencies."
2. **Linking appears broken.** Choosing "Parent Epic" in the benchwork link modal writes a valid
   `ParentEpic` edge, but because the parent has no outgoing edge, the freshly created link is
   invisible on reload. The write succeeds; the read never shows it.
3. **Dependents are invisible.** A `BlockedBy` edge surfaces on the blocked task only. The blocker
   never learns which tasks it is holding back, and siblings that recorded a mutual `RelatedTo` link
   on their own side are invisible to the other endpoint.

---

## 2. Technical Blueprint

1. In `crates/storypole-core/src/dag/briefing.rs` (`build_task_briefing`):
   * Include `EdgeKind::ParentEpic` alongside `RelatedTo`/`BlockedBy` in the outgoing pass so a
     child still lists its parent epic (and keeps inheriting resolved parent decisions).
   * Add a second, **inbound** pass over `all_tasks`: any task whose edge targets this task with one
     of the three summarised kinds is added to `related_tasks_summary`.
   * Deduplicate with a `Vec<Uuid>` of already-linked ids so mutual links collapse to one entry.
2. Wire shape stays `Vec<(String, String, TaskStatus)>` (documented in `CONTRACTS.md`); no schema
   change, so benchwork's TypeScript tuple type remains valid.
3. Regression tests in `crates/storypole-core/tests/briefing_edges_tests.rs`:
   * epic lists its children while each child still lists the epic;
   * `BlockedBy` surfaces on both endpoints;
   * mutual `RelatedTo` links are not duplicated;
   * a child briefing still reaches its ready parent epic.

---

## 3. Execution Steps

* **Step 1:** Extend briefing link summarisation with `ParentEpic` and inbound edges. Gate: `cargo check -p storypole-core`.
* **Step 2:** Add bidirectional regression tests. Gate: `cargo test -p storypole-core --test briefing_edges_tests`.
* **Step 3:** Tier 2 verification. Gate: `cargo nextest run -p storypole-core` + `cargo clippy -p storypole-core --all-targets -- -D warnings`.
* **Step 4:** Documentation + certification. Gate: `chalkline docs check -d storypole`.

---

## 4. Boundaries

* Task **ordering** semantics (`dag::mod` acyclic rules) are untouched — this is a read-model fix.
* Rendering changes belong to benchwork; `InspectorEdges.tsx` already consumes the tuple list and
  needs no change.
* Any further benchwork contract mismatch (e.g. `TaskStatus` casing) stays with `GB-97`.
