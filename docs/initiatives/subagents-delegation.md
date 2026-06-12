# Initiative 4: Sub-Agents & Delegation (+ Work Queue)

## Goal

Let a codel00p agent spawn focused sub-agents for parallel or isolated
workstreams, and coordinate long-horizon multi-agent work through a durable work
queue — without losing session, memory, permission, or audit coherence.

## Why (Hermes reference)

- **Delegation** (`tools/delegate_tool.py`): `delegate_task` spawns isolated
  subagents with their own terminal sessions. Synchronous — the parent waits for
  the child's summary. Two roles: `leaf` (focused worker, no further delegation)
  and `orchestrator` (can spawn workers). Batch delegation runs concurrently,
  capped by `delegation.max_concurrent_children`.
- **Kanban** (`hermes_cli/kanban.py`, `plugins/kanban/`): SQLite-backed
  multi-agent work queue. Workers atomically claim and complete tasks; a
  long-running dispatcher assigns work. After `kanban.failure_limit` consecutive
  non-success attempts, the dispatcher auto-blocks a task to prevent spin loops.
  Dashboard UI included.

## Current codel00p state

- Single agent loop in `codel00p-harness` (`AgentHarness`, `run_turn` /
  `run_turn_with_state`). No sub-agent spawning.
- This is **already on the roadmap**: Milestone 7 / Stage 7 "Multi-Agent Work"
  lists research/implementation/review/memory-extraction agents, worktree
  isolation for mutating subagents, parallel task execution, durable task plans,
  resumable long-running goals, and team-visible task state. This initiative is
  the concrete design for that milestone.
- `codel00p-session` already provides durable metadata with a `parent_id` field
  — the hook a sub-agent session tree needs.

## Design

### Delegation tool

- A `delegate_task` tool registered through the plugin registry
  ([#1](plugins-and-hooks.md)), available only to agents in an `orchestrator`
  role.
- Child runs as a fresh harness session with `parent_id` set (reuse existing
  `codel00p-session` lineage), its own context, and an inherited-or-narrowed
  permission scope (a child can never exceed the parent's scope).
- Roles: `leaf` (no `delegate_task` tool registered) and `orchestrator`.
- Synchronous summary return by default (matches Hermes); async/background
  variant later, coordinated with [#5 Scheduling](scheduling-cron.md).
- Batch delegation: spawn N children concurrently, capped by
  `delegation.max_concurrent_children` (config-layered).

### Worktree isolation (from roadmap Stage 7)

- Mutating sub-agents run in an isolated git worktree so parallel children do not
  collide on the working tree; results merge back through a review step.
- Read-only children skip worktree overhead.

### Event & audit coherence

- Child sessions emit the same `AgentEvent` stream tagged with their session id +
  `parent_id`, so the cloud/desktop timelines can render the agent tree.
- Memory candidates from children flow into the same project review queue.

### Work queue (kanban)

- A `codel00p-tasks` store on `codel00p-storage` (SQLite now, cloud backend
  later) holding task records: id, project, state
  (`pending`/`claimed`/`done`/`blocked`), assignee, attempts, parent goal.
- Atomic claim/complete using the storage append-log + state primitives.
- A dispatcher loop (a scheduled job, [#5](scheduling-cron.md)) assigns work and
  auto-blocks after a `failure_limit` of consecutive non-success attempts.
- `codel00p tasks` CLI surface; team-visible state via `codel00p-cloud`;
  desktop/cloud board UI in [#9 TUI](tui.md) / Stage 6 interfaces.

### Specialized agent roles (roadmap Stage 7)

- Research, implementation, review, and memory-extraction presets — a "review
  agent that can block or annotate risky work" composes naturally with the
  permission policy (it can veto a `pre_tool` decision).

## Scope

### Phase 1 — Synchronous delegation
- [ ] `delegate_task` tool, `leaf`/`orchestrator` roles, scope inheritance.
- [ ] Child sessions with `parent_id`; merged event stream + audit.
- [ ] `delegation.max_concurrent_children` batch execution.

### Phase 2 — Worktree isolation
- [ ] Worktree-isolated execution for mutating children; merge-back review.

### Phase 3 — Durable work queue
- [ ] `codel00p-tasks` store + atomic claim/complete.
- [ ] Dispatcher (scheduled) with `failure_limit` auto-block.
- [ ] `codel00p tasks` CLI; team-visible state in cloud.

### Phase 4 — Specialized roles & resumable goals
- [ ] Research/impl/review/memory agent presets.
- [ ] Durable, resumable long-horizon goal plans.

## Risks & open questions

- **Permission inheritance**: a child must never escalate beyond its parent; the
  scope-narrowing rule needs tests.
- **Cost/runaway control**: concurrent children multiply spend; reuse
  `--max-iterations` per child and add per-goal budget caps (ties to cost
  tracking, currently absent in codel00p).
- **Determinism**: concurrent children make event ordering nondeterministic;
  the session tree must remain replayable per-branch.
- **Sync vs async**: start synchronous (simpler, matches Hermes); async needs
  the scheduler and a notification surface.

## Dependencies

- [#1 Plugins & Hooks](plugins-and-hooks.md) (delegate tool seam, review-agent
  veto hook).
- [#5 Scheduling](scheduling-cron.md) (dispatcher loop, async delegation).
- Existing `codel00p-session` lineage and `codel00p-harness`.

## Exit criteria

- An orchestrator agent can split a large task across focused, permission-scoped
  children (some in isolated worktrees), coordinate them through a durable
  failure-aware queue, and preserve one coherent, replayable session/memory/audit
  record visible to the team.
