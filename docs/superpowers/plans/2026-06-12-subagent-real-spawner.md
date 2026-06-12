# Sub-Agent Real Spawner

Second slice of [Initiative 4: Sub-Agents & Delegation](../../initiatives/subagents-delegation.md),
Phase 1. Makes `delegate_task` actually run a child agent.

## Goal

Wire a CLI `SubAgentSpawner` that builds and runs a real child `AgentHarness`
for a delegated task, and let an orchestrator opt in via a `delegate` tool-set.

## Scope

- [x] `CliSubAgentSpawner`: reuses the orchestrator's provider config
      (registry/provider/model/base-url/preset) to build a child model client,
      runs a fresh child harness turn with the delegated task, and returns the
      child's assistant message + session id + tool-call count.
- [x] New `delegate` agent tool-set (`--tool-set delegate`, or
      `agent.tool_sets`): when present, `build_agent_harness` registers the
      `delegate_task` tool backed by the spawner (orchestrator role).
- [x] Children run with read-only tools and as leaves (no nested delegation),
      bounding blast radius and depth for this first wiring.
- [x] Test: the spawner runs a child against a mock chat-completions provider
      and returns its summary (single inference, deterministic).
- [x] Help and `configuration.md` list the `delegate` tool-set.
- [x] `cargo test -p codel00p-cli` (single-threaded — the parallel MCP test is
      timing-sensitive), `cargo fmt --check`, `cargo clippy`.

## Decisions

- **Opt-in via tool-set.** Reusing the existing `--tool-set` mechanism makes
  "this agent may delegate" a single, layered, auditable switch, with no new
  flag or role concept in the CLI. `all` deliberately does **not** include
  `delegate` — delegation is a distinct, higher-privilege opt-in.
- **Children are read-only leaves for now.** The spawner gives children
  `read_only_defaults` and no delegate tool. Narrowing a child to the parent's
  *actual* permission ceiling (rather than a fixed read-only set) is the next
  slice; it needs a scope lattice the protocol does not yet expose.
- **Provider config reuse.** The child rebuilds a provider client from the same
  (plugin-folded) registry and provider/model, so children route exactly like
  the parent. `ProviderRegistry` is `Clone`, so the spawner owns its own copy.

## Out of scope (next slices)

- Narrow child scope to the parent ceiling (vs fixed read-only).
- Child sessions tagged with `parent_id` and a merged parent/child event stream
  (the lineage primitive exists via `SessionMetadata::with_parent`).
- Concurrent batch delegation (`delegation.max_concurrent_children`).
- Worktree isolation for mutating children; the kanban work queue.
