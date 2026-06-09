# codel00p-harness

Rust agent runtime for codel00p.

This crate owns the first read-only harness loop:

- session state;
- workspace-safe file access;
- deterministic read-only tools;
- concurrency-safe tool batching with deterministic result order;
- model-client abstraction for fake and real inference;
- adapter to `codel00p-providers`;
- typed project memory context from `codel00p-memory`;
- opt-in candidate memory extraction after completed turns;
- typed events;
- optional event sinks for live progress;
- blocking context-window compaction into session-summary messages;
- bounded tool-call iteration.

Session IDs, turn IDs, session messages, agent events, and model tool calls are
shared from `codel00p-protocol`.

The harness does not own memory persistence. It accepts an explicit
`ProjectMemoryProvider`, retrieves approved project memory before inference,
and passes compact typed memory items to the model request. It can also accept
an explicit `TurnMemoryExtractor` and `MemoryCandidateSink` to write reviewable
candidate memory after a successful turn. Candidate extraction never approves
memory automatically. File editing, shell execution, cloud sync, and approvals
still belong behind explicit contracts so the runtime remains testable and
auditable.

Read-only default tools opt in to concurrency-safe execution. Custom tools are
serial unless they explicitly declare that a given input can run concurrently.

When a configured `ContextWindowState` reaches its blocking threshold, the
harness runs `on_pre_compact`, summarizes older transcript messages into one
system message, preserves recent messages, and emits `context_compacted`.
Callers that need live progress can attach an `AgentEventSink`; it receives the
same event sequence returned in `TurnOutcome.events`.

Editing tools are available through `ToolRegistry::editing_defaults()`:

- `create_file`;
- `update_file`;
- `delete_file`;
- `apply_patch`.

These tools all require `PermissionScope::WorkspaceWrite`. They are not part of
`ToolRegistry::read_only_defaults()`.

Command execution is available through `ToolRegistry::command_defaults()`:

- `run_command`.

`run_command` requires `PermissionScope::Shell`, runs inside the workspace,
accepts `program` plus `args` instead of a free-form shell string, and returns
structured exit status, timeout, stdout, stderr, and truncation fields.

Git workflow tools are available through `ToolRegistry::git_defaults()`:

- `git_status`;
- `git_diff`;
- `git_log`;
- `git_commit`.

Read-only git tools require `PermissionScope::ReadOnly`. `git_commit` requires
`PermissionScope::WorkspaceWrite`, rejects `Co-authored-by:` trailers, refuses
to commit while untracked files are present, and stages tracked changes with
`git add -u`.

## Project Instructions

The harness loads root-level project instruction files before each model call.
Precedence is deterministic:

1. `CODEL00P.md`
2. `AGENTS.md`
3. `CLAUDE.md`

The assembled instruction block is sent as the first system message. Approved
project memory is sent after instructions and before session messages.

## Minimal Shape

```rust
let outcome = AgentHarness::builder()
    .model_client(model_client)
    .workspace(workspace)
    .tools(ToolRegistry::read_only_defaults())
    .max_iterations(4)
    .build()?
    .run_turn(session_id, UserMessage::new("Inspect the project."))
    .await?;
```

Persisted sessions can be resumed by rebuilding `SessionState` from durable
message records and using `run_turn_with_state`:

```rust
let outcome = AgentHarness::builder()
    .model_client(model_client)
    .workspace(workspace)
    .tools(ToolRegistry::read_only_defaults())
    .build()?
    .run_turn_with_state(
        restored_session_state,
        UserMessage::new("Continue with the next step."),
    )
    .await?;
```

Project memory can be supplied through a repository-backed provider:

```rust
let provider = MemoryRepositoryProjectMemoryProvider::new(project, memory_store)
    .with_kind(MemoryKind::Architecture)
    .with_tag("harness")
    .with_text("tool execution")
    .with_limit(8);

let outcome = AgentHarness::builder()
    .model_client(model_client)
    .workspace(workspace)
    .project_memory_provider(provider)
    .build()?
    .run_turn(session_id, UserMessage::new("Inspect the project."))
    .await?;
```

Approved memory is assembled into a provider-neutral system message before
session messages:

```text
Project memory:
- id: mem-harness
  kind: architecture
  tags: harness,runtime
  reason: matched tag harness
  content: The harness owns tool execution.
```

Candidate memory can be extracted after completed turns:

```rust
let extractor = ExplicitTurnMemoryExtractor::new(project.clone()).with_tag("assistant");
let sink = MemoryRepositoryCandidateSink::new(memory_store);

let outcome = AgentHarness::builder()
    .model_client(model_client)
    .workspace(workspace)
    .turn_memory_extractor(extractor)
    .memory_candidate_sink(sink)
    .build()?
    .run_turn(session_id, UserMessage::new("Summarize what to remember."))
    .await?;
```

Extractor and sink failures are non-fatal and are recorded as
`LifecycleHookFailed` events with `memory_extraction` or
`memory_candidate_sink` hook names.

Tests use fake model clients. Live provider behavior stays in
`codel00p-providers`.
