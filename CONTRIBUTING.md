# Contributor Guide — `storypole`

> Guidelines for engineering, testing, and contributing to the `storypole` codebase.

---

## 1. Toolchain & Environment Setup

`storypole` is built with the **Rust 2024 edition**:

```bash
# Verify rust toolchain
rustc --version # Rust 1.85+ (2024 edition compatible)

# Install cargo-nextest (preferred test runner)
cargo install cargo-nextest --locked
```

Clone the repository and compile member crates:

```bash
git clone https://github.com/gaugeblock/storypole.git
cd storypole
cargo check --workspace --all-targets
```

---

## 2. Workspace Organization

```text
storypole/
├── src/                       # Root facade re-exporting storypole-core
└── crates/
    ├── storypole-core/        # Pure domain types, SQLite projection, DAG traverser
    ├── storypole-cli/         # Clap CLI binary and command handlers
    └── storypole-mcp/         # Model Context Protocol JSON-RPC 2.0 stdio server
```

---

## 3. Tiered Verification Workflow

All code contributions must satisfy the tiered verification matrix before merge:

### Tier 1 (Inner Loop)
Rapid check during crate modification:
```bash
cargo check -p storypole-core
cargo nextest run -p storypole-core --lib
```

### Tier 2 (Commit Gate)
Run on affected crates prior to making an atomic commit:
```bash
cargo fmt --check -p storypole-core
cargo clippy -p storypole-core --all-targets -- -D warnings
cargo nextest run -p storypole-core
chalkline docs check -d .
```

### Tier 3 (Release Gate)
Full workspace regression check:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
chalkline docs check --workspace
```

---

## 4. Coding Standards & Invariants

1. **Hard 250-Line File Limit:** No `.rs`, `.ts`, or `.tsx` file may exceed 250 lines. Decompose proactively into submodules.
2. **Strict Error Handling:** Library crates (`storypole-core`) must use `thiserror`. Never use `.unwrap()` or `.expect()` in non-test library code (`[workspace.lints.clippy]` `unwrap_used = "deny"`).
3. **Structured Telemetry:** Use `tracing` for logging. Never print unstructured lines to `stdout` in MCP or JSON mode; stdout is strictly reserved for protocol frames.
4. **Conventional Commits:** `<type>(<scope>): <summary>` (e.g. `feat(storypole-core): add cycle detection to link command`).
