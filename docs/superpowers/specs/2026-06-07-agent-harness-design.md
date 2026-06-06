# Agent Harness Design

## Goal

Build `codel00p-harness`: a Rust agent runtime that runs session turns,
dispatches read-only tools, calls `codel00p-providers`, emits traceable events,
and exposes clean hooks for future project memory.

## Architecture

The harness is a custom Rust crate inspired by Hermes Agent. Hermes is the
reference for conversation loop, tool registry, session state, provider routing,
and event tracing, but codel00p should own the runtime contract directly.

The harness depends on `codel00p-providers` for inference. It does not own
provider routing, credentials, or model-specific request behavior.

The first implementation is read-only. It supports file listing, file reading,
and text search inside a workspace root. Editing and shell execution are future
work because they require approvals, patch safety, rollback, and stronger audit
semantics.

## Public API

```rust
let mut harness = AgentHarness::builder()
    .providers(provider_client)
    .workspace(workspace)
    .tools(ToolRegistry::read_only_defaults())
    .build();

let outcome = harness
    .run_turn(
        SessionId::new(),
        UserMessage::new("Inspect the project and summarize what changed."),
    )
    .await?;
```

The harness returns structured output:

```rust
pub struct TurnOutcome {
    pub assistant_message: Option<String>,
    pub tool_calls: Vec<ExecutedToolCall>,
    pub events: Vec<HarnessEvent>,
    pub session_state: SessionState,
}
```

## Crate Boundaries

```text
crates/codel00p-harness/
  src/
    lib.rs
    agent.rs
    approvals.rs
    context.rs
    errors.rs
    events.rs
    session.rs
    tool_registry.rs
    tool_result.rs
    tools.rs
    turn.rs
    workspace.rs
```

`agent.rs` exposes the facade. `turn.rs` owns the loop. `session.rs` owns
messages and state. `tools.rs` owns the trait and read-only built-ins.
`workspace.rs` owns root safety. `events.rs` owns the event stream.

## Turn Loop

1. Create a turn ID.
2. Append the user message to session state.
3. Build context from current session and context providers.
4. Send an `InferenceRequest` through `codel00p-providers`.
5. If the response has tool calls, dispatch each through the tool registry.
6. Append tool results to session state.
7. Repeat until the model returns assistant text with no tool calls.
8. Stop early if the iteration budget is reached.
9. Return `TurnOutcome`.

## Initial Tools

The first tool set is read-only:

- `list_files`;
- `read_file`;
- `search_text`.

All tool paths are resolved through `Workspace`, which rejects path traversal
and any path outside the configured root.

## Events

The harness emits deterministic events:

- `SessionStarted`;
- `TurnStarted`;
- `ContextBuilt`;
- `InferenceRequested`;
- `InferenceCompleted`;
- `ToolCallRequested`;
- `ToolCallCompleted`;
- `ToolCallFailed`;
- `TurnCompleted`.

The event stream is part of the public product contract. CLI, desktop, cloud,
logs, and replay should use it instead of parsing terminal text.

## Memory Hooks

The harness does not implement memory. It exposes interfaces:

```rust
#[async_trait::async_trait]
pub trait ContextProvider {
    async fn build_context(&self, request: ContextRequest) -> Result<ContextBundle, HarnessError>;
}

#[async_trait::async_trait]
pub trait MemorySink {
    async fn observe_turn(&self, outcome: &TurnOutcome) -> Result<(), HarnessError>;
}
```

`codel00p-memory` can later implement these traits.

## Testing

Harness tests use fake providers, not live models.

Required behavior:

- final assistant response without tools;
- tool call execution;
- unknown tool handling;
- iteration limit;
- deterministic event order;
- workspace path safety;
- session state accumulation;
- session state serialization.

Live provider behavior remains covered in `codel00p-providers`.

## Non-Goals

- editing files;
- running shell commands;
- direct Hermes dependency;
- memory extraction;
- cloud sync;
- provider credential resolution.

