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
