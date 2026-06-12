# Sub-Agent Delegation Core

First slice of [Initiative 4: Sub-Agents & Delegation](../../initiatives/subagents-delegation.md),
Phase 1. Establishes the in-harness delegation seam without yet running a real
child agent.

## Goal

Let an orchestrator agent hand a focused task to a child via a `delegate_task`
tool, behind a `SubAgentSpawner` seam, with leaf agents structurally unable to
delegate.

## Scope

- [x] `codel00p-harness` `delegation` module:
  - `AgentRole` (`Leaf` / `Orchestrator`) with `can_delegate()`.
  - `DelegatedTask` (description + optional label) and `DelegationOutcome`
    (summary, child session id, tool-call count).
  - `SubAgentSpawner` trait — the seam an application implements to actually run
    a child harness.
  - `DelegateTaskTool` implementing `Tool`: parses `{task, label?}`, calls the
    spawner, returns the child's summary as JSON. Scope `ExternalConnector`.
  - `delegation_tools(role, spawner)` — returns the `delegate_task` tool only for
    orchestrators; a leaf gets an empty registry (this is what bounds depth).
- [x] Tests with a stub spawner: role gating, spawner invocation + summary
      return, missing-task error.
- [x] `cargo test -p codel00p-harness`, `cargo fmt --check`, `cargo clippy`.

## Decisions

- **Seam first, no provider.** How a child harness is built — model client, tool
  set, narrowed permissions, worktree isolation — lives behind `SubAgentSpawner`
  and is wired by the application in the next slice. This keeps delegation
  testable without a real provider, mirroring the plugin-registry slices.
- **Depth is bounded by role, not a counter.** A leaf simply has no
  `delegate_task` tool, so it cannot spawn children. An orchestrator's children
  are spawned as leaves by the (future) real spawner.
- **`ExternalConnector` scope for now.** Spawning is gated like a connector;
  adding a dedicated `PermissionScope::Delegation` would touch the protocol and
  its golden-JSON fixtures, so it is deferred to its own slice.

## Out of scope (next slices)

- The real spawner: build + run a child `AgentHarness` with a narrowed
  permission ceiling, wired from the CLI provider/model config.
- Child sessions with `parent_id` (the lineage primitive already exists via
  `SessionMetadata::with_parent`) and a merged parent/child event stream.
- Concurrent batch delegation (`delegation.max_concurrent_children`).
- Worktree isolation for mutating children; the kanban work queue.
- A dedicated `Delegation` permission scope.
