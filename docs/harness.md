# Agent Harness

`codel00p-harness` is the Rust agent runtime for codel00p. It owns the session
loop, tool execution, event stream, workspace boundary, and integration with
`codel00p-providers`.

The first implementation now exists under
[`core/crates/codel00p-harness`](../core/crates/codel00p-harness). It supports
deterministic read-only turns, fake model-client tests, workspace-safe tools,
bounded tool-call loops, concurrency-safe tool batching, a provider adapter, and
shared public contracts from `codel00p-protocol`.
It also exposes permission policy and lifecycle hook surfaces so future memory,
compaction, approvals, and remote-control modules do not have to fork the core
turn loop.

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

Consecutive tools marked as concurrency-safe may run in the same batch. Unsafe
or unknown tools always execute serially and split adjacent safe batches. The
harness preserves the model's tool-call order when recording events, tool
results, and session messages.

## Public Interface

The harness should be easy to embed:

```rust
let mut harness = AgentHarness::builder()
    .model_client(model_client)
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

Resumed sessions use the same runtime after the caller restores prior message
records:

```rust
let outcome = harness
    .run_turn_with_state(
        restored_session_state,
        UserMessage::new("Continue from the previous turn."),
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
core/crates/codel00p-harness/
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

Read-only defaults are marked concurrency-safe. Custom tools must explicitly opt
in by implementing `Tool::is_concurrency_safe`; the default is serial execution.
Custom tools also expose a `PermissionScope`. Unknown/custom tools default to a
write-like scope so policy must explicitly allow them before execution.

## Editing Tools

The harness now exposes an opt-in editing tool set through
`ToolRegistry::editing_defaults()`. These tools are not included in
`ToolRegistry::read_only_defaults()`, so CLI runs stay read-only unless the user
passes `--tool-set edit` or `--tool-set all`.

Editing tools:

- `create_file`: create a new UTF-8 file under the workspace root;
- `update_file`: replace an existing UTF-8 file under the workspace root;
- `delete_file`: delete an existing file under the workspace root;
- `apply_patch`: apply exact string replacements to existing files.

All editing tools declare `PermissionScope::WorkspaceWrite`. The agent loop
requests permission before execution and records denied writes as structured
tool results without mutating the workspace.

The first `apply_patch` implementation intentionally uses exact replacements
instead of a full unified-diff parser. That keeps patch behavior deterministic
and testable while leaving room for a richer patch format later.

## Command Execution

The harness exposes shell execution through `ToolRegistry::command_defaults()`.
The first command tool is:

- `run_command`: run a program inside the workspace with timeout and output
  limits.

`run_command` accepts a structured argument vector instead of a free-form shell
string:

```json
{
  "program": "cargo",
  "args": ["test", "--workspace"],
  "cwd": ".",
  "timeout_ms": 30000,
  "max_output_bytes": 16384
}
```

The tool declares `PermissionScope::Shell`. The agent loop requests permission
before execution. Denied shell commands are returned to the model as structured
permission-denied tool results without executing the process.

CLI runs opt into this tool set with `--tool-set command` or `--tool-set all`.

Command results are structured:

```json
{
  "exit_code": 0,
  "success": true,
  "timed_out": false,
  "stdout": "...",
  "stderr": "...",
  "stdout_truncated": false,
  "stderr_truncated": false
}
```

The working directory is constrained to the workspace root. Timeouts and output
caps are bounded even when the model requests larger values.

## Git Workflow Tools

The harness exposes guarded git tools through `ToolRegistry::git_defaults()`:

- `git_status`: stable porcelain status, read-only;
- `git_diff`: tracked-file diff with output caps, read-only;
- `git_log`: recent commit metadata, read-only;
- `git_commit`: guarded commit creation for tracked changes.

`git_status`, `git_diff`, and `git_log` declare `PermissionScope::ReadOnly`.
`git_commit` declares `PermissionScope::WorkspaceWrite`.

The first commit implementation is intentionally strict:

- commit messages must not contain `Co-authored-by:` trailers;
- untracked files cause the commit to fail;
- only tracked changes are staged with `git add -u`;
- the result reports the new commit SHA, subject, changed-file count, and git
  output.

This gives the agent a safe base for clean commits without taking ownership of
untracked user work.

CLI runs opt into git tools with `--tool-set git` or `--tool-set all`.

## Runtime Control

The harness has a control layer inspired by Hermes and Claude Code:

- `PermissionPolicy` receives a typed `PermissionRequest` before each tool call.
- denied tools are not executed; they become structured tool results with
  `permission_denied`;
- permission request/denial and tool progress events are emitted through the
  shared protocol event stream;
- `ContextWindowState` can be attached to inference requests so context engines,
  UI, and provider routing see the same pressure information.

## Project Instructions

The harness loads repository instruction files before each inference request.
The first supported scope is the workspace root, with deterministic precedence:

1. `CODEL00P.md`
2. `AGENTS.md`
3. `CLAUDE.md`

`CODEL00P.md` is the native codel00p instruction file. `AGENTS.md` and
`CLAUDE.md` are compatibility inputs so existing repositories can migrate
without losing their agent rules.

When present, instructions become the first provider system message:

```text
Project instructions:
## CODEL00P.md
Run pnpm verify before pushing main.

## AGENTS.md
Follow repository coding conventions.
```

Project memory is still injected after project instructions and before session
messages. This ensures the model sees durable repository rules before retrieved
facts and prior conversation.

## Lifecycle Hooks

`LifecycleHook` is the memory/context extension point. Hooks currently run at:

- turn start;
- pre-inference;
- post-tool;
- pre-compact;
- turn completion.

Hook failures are non-fatal and become `LifecycleHookFailed` events. This keeps
memory extraction, compaction bookkeeping, telemetry, and remote sidecars from
crashing the agent loop when they fail.

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

The harness exposes explicit read and write memory boundaries:

```rust
#[async_trait::async_trait]
pub trait ProjectMemoryProvider {
    async fn retrieve(&self, request: ProjectMemoryRequest) -> Result<ProjectMemoryContext, HarnessError>;
}

#[async_trait::async_trait]
pub trait TurnMemoryExtractor {
    async fn extract(&self, request: TurnMemoryExtractionRequest) -> Result<Vec<MemoryCandidateInput>, HarnessError>;
}

#[async_trait::async_trait]
pub trait MemoryCandidateSink {
    async fn persist(&self, candidates: Vec<MemoryCandidateInput>) -> Result<MemoryCandidateSinkOutcome, HarnessError>;
}
```

`ProjectMemoryProvider` supplies approved memory before inference.
`TurnMemoryExtractor` and `MemoryCandidateSink` are opt-in and run after a
successful final assistant response. They create candidate memory only; the CLI,
desktop app, or cloud review workflow must approve candidates before retrieval
can use them. Extraction and sink failures are non-fatal and become
`LifecycleHookFailed` events.

Approved project memory is assembled into a provider-neutral system message
before session messages:

```text
Project memory:
- id: mem-harness
  kind: architecture
  tags: harness,runtime
  reason: matched tag harness
  content: The harness owns tool execution.
```

The prompt block is sorted by memory id for deterministic provider requests and
includes retrieval reasons so users can inspect why the memory was loaded.

## Test Strategy

Use TDD from the first harness commit.

Required tests:

- a user turn can return a final assistant message from a fake provider;
- a tool call from a fake provider executes a registered tool;
- an unknown tool produces a controlled tool error;
- concurrency-safe tools run in parallel batches;
- unsafe tools split parallel batches;
- permission denials produce structured tool results without executing tools;
- lifecycle hook failures emit events and do not abort the turn;
- iteration budget stops infinite tool-call loops;
- events are emitted in deterministic order;
- workspace paths cannot escape the workspace root;
- session state accumulates user, assistant, and tool messages;
- session state serializes and deserializes without losing messages.

Live provider tests should stay in `codel00p-providers`. Harness tests should
use fake providers so the harness loop remains deterministic.
