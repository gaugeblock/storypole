# AGENTS.md — Operational Directives for `storypole`

> **Organization:** `gaugeblock` (`https://github.com/gaugeblock` | `https://gaugeblock.dev`)  
> **Crates:** `storypole`, `storypole-core`, `storypole-cli`, `storypole-mcp`  
> **Suite Lifecycle:** `0.1.0-beta.X`

You are an autonomous systems engineering agent operating inside `storypole`. All operations must adhere to these non-negotiable engineering directives.

---

## 1. Non-Negotiable Engineering Laws

1. **Hard 250-Line File Limit:**
   * No `.rs`, `.ts`, or `.tsx` source file may exceed **250 lines of code** (including comments and tests).
   * If a file approaches 200 lines, decompose immediately into focused submodules (`mod.rs`, `types.rs`, `handlers.rs`) and move unit tests into a sibling `tests.rs` or `tests/` directory.
2. **Unix Philosophy & Standalone Value:**
   * `storypole` must compile, pass tests, and provide a genuinely useful CLI and MCP backlog experience completely on its own without requiring the rest of the `gaugeblock` suite to be running.
3. **Zero Inline Scope Creep (storypole defer):**
   * While executing any task, if you discover an unrelated bug, edge case, or refactoring opportunity outside your immediate task scope, **DO NOT fix it inline**.
   * You MUST record the item immediately via `storypole defer --kind <bug|tech-debt|enhancement> --symbol "<crate::path>" --title "..."` (or the `storypole_defer` MCP tool). Stay locked on your active task.
4. **Tiered Verification Matrix:**
   * **Tier 1 (Inner Loop):** `cargo check -p storypole-core` or targeted test `cargo nextest run -p storypole-core --lib <submodule>`.
   * **Tier 2 (Commit Gate):** `cargo fmt --check -p <crate>`, `cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo nextest run -p <crate>`, and `chalkline docs check -d .` (certifying 9-doc compliance and zero AST drift).
   * **Tier 3 (Task Completion Gate):** Full workspace check: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run --workspace`, and `chalkline docs check --workspace`.
5. **Strict Error Handling & Observability:**
   * Library crates (`storypole-core`) MUST use `thiserror` with strongly typed error enums. `.unwrap()` and `.expect()` are strictly forbidden in production code (`unwrap_used = "deny"`).
   * Use `tracing` for structured telemetry. Never write unstructured logs to `stdout` in MCP or JSON mode; all logs route to `stderr`.
6. **Atomic Commits & Bisect Invariance:**
   * Every commit must represent a single logical unit of work and pass Tier 2 verification. Never break `git bisect`.
7. **Conventional Commits Specification:**
   * Commit messages MUST follow Conventional Commits v1.0.0: `<type>(<scope>): <summary>`
   * Types: `feat`, `fix`, `refactor`, `perf`, `test`, `docs`, `style`, `chore`, `ci`, `build`.
   * Scopes: `storypole-core`, `storypole-cli`, `storypole-mcp`, or `workspace`.
   * Footers: Reference task keys (e.g. `Task: GB-109`).
8. **Semantic Versioning & Git Tagging:**
   * Member crates lockstep via repository root `Cargo.toml`. Tags follow `v<version>`.
9. **Plan Persistence & Specification Fidelity:**
   * Every non-trivial task requires an attached `TaskPlan` (`storypole plan set`) and markdown plan saved under `.storypole/plans/<KEY>.md`.
10. **Branch Isolation & Claim Leases:**
   * All work proceeds on feature branches (`feat/<ticket-or-slug>`). Use `storypole claim <KEY> <AGENT_ID> [--branch <BRANCH>]` to record branch and commit SHAs.
