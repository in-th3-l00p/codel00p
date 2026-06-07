# Agent Harness

`codel00p-harness` is the Rust agent runtime for codel00p. It owns the session
loop, tool execution, event stream, workspace boundary, and integration with
`codel00p-providers`.

The first implementation now exists under
[`crates/codel00p-harness`](../crates/codel00p-harness). It supports
deterministic read-only turns, fake model-client tests, workspace-safe tools,
bounded tool-call loops, a provider adapter, and shared public contracts from
`codel00p-protocol`.

The first harness milestone should be intentionally small: read-only repository
work, deterministic tests, and a clean public interface that future CLI,
desktop, cloud, and memory modules can share.

## Direction

Use Hermes Agent as an architecture reference, not as a direct dependency.

Hermes proves the right concepts:

- conversation loop;
- provider routing;
- tool registry;
- tool execution;
- session state;
- event tracing;
- memory hooks;
- context assembly.

codel00p should own its harness contract because project memory, organization
policy, cloud coordination, and module boundaries are product-level concerns.

## First Milestone

The minimal read-only harness:

1. accept a user turn;
2. build context from session state and workspace metadata;
3. call `codel00p-providers`;
4. execute model-requested read-only tools;
5. append tool results to session state;
6. continue until final assistant text or iteration limit;
7. emit typed events for every important step;
8. return a structured `TurnOutcome`.

Editing tools remain deliberately excluded from the first milestone. File mutation
needs approvals, patch safety, rollback, and stronger audit behavior.

## Public Interface

The harness should be easy to embed:

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

The outcome should be structured:

```rust
pub struct TurnOutcome {
    pub assistant_message: Option<String>,
    pub tool_calls: Vec<ExecutedToolCall>,
    pub events: Vec<HarnessEvent>,
    pub session_state: SessionState,
}
```

## Crate Layout

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

Responsibilities:

- `agent.rs`: high-level `AgentHarness` facade and builder.
- `session.rs`: session IDs, messages, and serializable session state.
- `turn.rs`: turn execution loop and iteration budget.
- `events.rs`: typed event stream.
- `context.rs`: context assembly and memory hook traits.
- `tools.rs`: `Tool` trait and built-in read-only tools.
- `tool_registry.rs`: deterministic tool registration and lookup.
- `tool_result.rs`: normalized tool result shape.
- `workspace.rs`: repository root boundary and path safety.
- `approvals.rs`: approval types reserved for later editing tools.
- `errors.rs`: typed harness errors.

## Initial Read-Only Tools

The first safe tool set:

- `list_files`: list files under the workspace root.
- `read_file`: read a UTF-8 file under the workspace root.
- `search_text`: search text under the workspace root.

`run_command` should not ship in the first harness milestone. It creates a much
larger security and determinism surface than file reading/searching.

## Event Stream

Every harness turn should emit deterministic events:

```rust
pub enum HarnessEvent {
    SessionStarted { session_id: SessionId },
    TurnStarted { session_id: SessionId, turn_id: TurnId },
    ContextBuilt { message_count: usize },
    InferenceRequested { provider: String, model: String },
    InferenceCompleted { finish_reason: Option<String> },
    ToolCallRequested { name: String },
    ToolCallCompleted { name: String },
    ToolCallFailed { name: String, message: String },
    TurnCompleted { iterations: u32 },
}
```

The event stream is a product boundary. CLI, desktop, cloud, debugging, audit,
and replay should all consume the same protocol event shape.

## Memory Boundaries

Do not implement memory inside the harness.

The harness should expose hooks:

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

`codel00p-memory` can later provide implementations. Until then, the harness
can use an empty context provider and no-op memory sink.

## Test Strategy

Use TDD from the first harness commit.

Required tests:

- a user turn can return a final assistant message from a fake provider;
- a tool call from a fake provider executes a registered tool;
- an unknown tool produces a controlled tool error;
- iteration budget stops infinite tool-call loops;
- events are emitted in deterministic order;
- workspace paths cannot escape the workspace root;
- session state accumulates user, assistant, and tool messages;
- session state serializes and deserializes without losing messages.

Live provider tests should stay in `codel00p-providers`. Harness tests should
use fake providers so the harness loop remains deterministic.
