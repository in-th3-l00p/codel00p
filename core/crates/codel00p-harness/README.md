# codel00p-harness

Rust agent runtime for codel00p.

This crate owns the first read-only harness loop:

- session state;
- workspace-safe file access;
- deterministic read-only tools;
- concurrency-safe tool batching with deterministic result order;
- model-client abstraction for fake and real inference;
- adapter to `codel00p-providers`;
- typed events;
- bounded tool-call iteration.

Session IDs, turn IDs, session messages, agent events, and model tool calls are
shared from `codel00p-protocol`.

The harness intentionally does not implement memory, file editing, shell
execution, cloud sync, or approvals yet. Those belong behind explicit contracts
so the runtime remains testable and auditable.

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

Tests use fake model clients. Live provider behavior stays in
`codel00p-providers`.
