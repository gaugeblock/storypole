# Behavioral Specifications & Invariants

## Formal Type Contracts

- **`args::AddArgs`** (`struct`): Public interface entity
- **`decision::AgentClaim`** (`struct`): Active agent claim lock with lease expiry semantics.
- **`args::AskDecisionArgs`** (`struct`): Public interface entity
- **`args::Cli`** (`struct`): Git-native backlog, task DAG, decision lifecycle engine, and MCP server.
- **`args::Commands`** (`enum`): Public interface entity
- **`mutations::CreateTaskParams`** (`struct`): Parameters for creating a new task.
- **`args::DecideArgs`** (`struct`): Public interface entity
- **`decision::DecisionStatus`** (`enum`): Lifecycle state for an attached decision.
- **`args::DeferArgs`** (`struct`): Public interface entity
- **`discovery::DiscoveredMember`** (`struct`): A member repository discovered within a workspace.
- **`task::EdgeKind`** (`enum`): Semantic relationship type between tasks in the DAG.
- **`args::HandoffArgs`** (`struct`): Public interface entity
- **`decision::HandoffPacket`** (`struct`): Structured continuity checkpoint recorded when an agent pauses, yields, or fails mid-task.
- **`args::InitArgs`** (`struct`): Public interface entity
- **`events::LedgerEvent`** (`enum`): Immutable append-only event written to `.storypole/ledger.jsonl`.
- **`args::ListArgs`** (`struct`): Public interface entity
- **`args::NextArgs`** (`struct`): Public interface entity
- **`plan_args::PlanArgs`** (`struct`): Manage attached execution plans and design markdown blueprints.
- **`plan_args::PlanCommands`** (`enum`): Public interface entity
- **`plan::PlanStep`** (`struct`): Granular step within an architectural or execution plan.
