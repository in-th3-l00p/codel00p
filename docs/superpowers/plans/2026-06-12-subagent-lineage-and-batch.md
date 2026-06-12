# Sub-Agent Lineage & Batch Delegation

Third slice of [Initiative 4: Sub-Agents & Delegation](../../initiatives/subagents-delegation.md),
completing Phase 1.

## Goal

Make delegated child runs auditable (recorded as sessions linked to their
parent) and let an orchestrator delegate a batch of tasks concurrently under a
configurable cap.

## Scope

- [x] Child sessions persisted with the orchestrator's session as `parent`
      (`SessionMetadata::with_parent`) and a `subagent` source. Factored
      `persist_turn_outcome` into `persist_session_records(source, parent)`.
- [x] `parent_session_id` threaded into `build_agent_harness` (both `agent run`
      and `agent chat` paths) and into the spawner.
- [x] `delegate_task` marked concurrency-safe, so a delegate batch runs
      concurrently; a shared `tokio::sync::Semaphore` in the spawner caps how
      many children run at once.
- [x] `delegation.max_concurrent_children` config key (layered `[delegation]`
      section, `config get/set/unset`), default 4.
- [x] Test: spawner runs a child against a mock provider, returns its summary,
      and the child session is persisted with the parent lineage. Harness test
      asserts `delegate_task` is concurrency-safe.
- [x] Docs: `configuration.md` documents the key and the `delegate` tool-set.
- [x] `cargo test` (harness + cli single-threaded), `cargo fmt --check`,
      `cargo clippy`.

## Decisions

- **Lineage via a linked session, not interleaved events.** A child is its own
  persisted session with `parent_id`; the parent's `delegate_task` tool result
  carries the `child_session_id`. This keeps each session's event stream clean
  while making the agent tree fully queryable (`session show`) — the "merged
  stream + audit" requirement, modelled as a graph rather than a flattened log.
- **Cap via semaphore, not by serializing.** Marking the tool concurrency-safe
  lets the harness fire a batch through its existing concurrent-tool path; the
  semaphore enforces `max_concurrent_children` regardless of batch size.

## Phase 1 is now complete

`delegate_task` exists (orchestrator-gated), runs real children, records lineage,
and batches under a cap. Remaining initiative work is later phases:

- Narrow child scope to the parent's *actual* ceiling (today children are
  read-only, which is conservatively within any ceiling).
- **Phase 2** — worktree isolation for mutating children.
- **Phase 3** — durable kanban work queue + dispatcher + `codel00p tasks`.
- **Phase 4** — specialized agent presets, resumable long-horizon goals.
