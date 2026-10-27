# AGENTS.md — Operational Directives for storypole

## 1. Non-Negotiable Engineering Laws

1. **Hard 250-Line File Limit:** No `.rs`, `.ts`, or `.tsx` source file may exceed 250 lines.
2. **Unix Philosophy:** Standalone crate value with clean abstractions.
3. **Zero Inline Scope Creep:** Defer discoveries to `storypole defer`.
4. **Tiered Verification:** Ensure Tier 1-3 gates pass.
