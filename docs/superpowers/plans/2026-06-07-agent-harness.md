# Agent Harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` or `superpowers:executing-plans` to
> implement this plan task by task. Track progress by updating the checkboxes in
> this file.

## Goal

Build the first `codel00p-harness` Rust crate: a deterministic read-only agent
runtime that can run a user turn, call inference through a small model-client
boundary, execute read-only workspace tools, emit typed events, and return a
structured outcome.

## Architecture

The harness owns agent control flow, session state, tool dispatch, workspace
safety, events, and memory hook points. It depends on `codel00p-providers`
through an adapter, but tests should use fake model clients so harness behavior
does not depend on live inference providers.

Initial scope:

- user and assistant session messages;
- read-only tools: `list_files`, `read_file`, `search_text`;
- deterministic tool registry;
- safe workspace-relative paths;
- typed event stream;
- turn loop with iteration budget;
- fake model client tests;
- adapter from `codel00p-providers::InferenceClient`.

Out of scope for the first crate milestone:

- file editing;
- shell command execution;
- approvals UI;
- cloud sync;
- memory extraction;
- direct Hermes dependency.

## Tech Stack

- Rust workspace crate: `crates/codel00p-harness`
- Runtime: `tokio`
- Async traits: `async-trait`
- Serialization: `serde`, `serde_json`
- Errors: `thiserror`
- Test fixtures: `tempfile`
- Existing inference crate: `codel00p-providers`

## Public API Target

```rust
let outcome = AgentHarness::builder()
    .model_client(fake_or_provider_client)
    .workspace(Workspace::new(repo_root)?)
    .tools(ToolRegistry::read_only_defaults())
    .max_iterations(4)
    .build()?
    .run_turn(SessionId::new(), UserMessage::new("Summarize this project."))
    .await?;
```

```rust
pub struct TurnOutcome {
    pub assistant_message: Option<String>,
    pub tool_calls: Vec<ExecutedToolCall>,
    pub events: Vec<HarnessEvent>,
    pub session_state: SessionState,
}
```

## Tasks

- [x] 1. Scaffold `codel00p-harness`

  Add the crate to the workspace and create an empty public module surface.

  Files:

  - `Cargo.toml`
  - `crates/codel00p-harness/Cargo.toml`
  - `crates/codel00p-harness/src/lib.rs`

  Initial exports:

  ```rust
  pub mod agent;
  pub mod context;
  pub mod errors;
  pub mod events;
  pub mod session;
  pub mod tool_registry;
  pub mod tool_result;
  pub mod tools;
  pub mod turn;
  pub mod workspace;
  ```

  Verification:

  ```bash
  cargo test -p codel00p-harness
  ```

  Commit:

  ```bash
  git commit -m "feat: scaffold harness crate"
  ```

- [x] 2. Add session and event types with tests first

  Write tests for session message accumulation, stable IDs, and JSON
  serialization before implementing the types.

  Files:

  - `crates/codel00p-harness/src/session.rs`
  - `crates/codel00p-harness/src/events.rs`
  - `crates/codel00p-harness/src/errors.rs`
  - `crates/codel00p-harness/tests/session_state.rs`

  Target types:

  ```rust
  pub struct SessionId(String);
  pub struct TurnId(String);

  pub enum SessionMessage {
      User { content: String },
      Assistant { content: String },
      Tool { call_id: String, name: String, content: String },
  }

  pub struct SessionState {
      pub session_id: SessionId,
      pub messages: Vec<SessionMessage>,
  }
  ```

  Verification:

  ```bash
  cargo test -p codel00p-harness --test session_state
  ```

  Commit:

  ```bash
  git commit -m "feat: add harness session state"
  ```

- [x] 3. Implement workspace path safety

  Write tests proving valid relative paths resolve under the root and traversal
  attempts fail.

  Files:

  - `crates/codel00p-harness/src/workspace.rs`
  - `crates/codel00p-harness/tests/workspace_paths.rs`

  Required behavior:

  - `Workspace::new(root)` canonicalizes an existing root;
  - `resolve("src/lib.rs")` stays inside the root;
  - `resolve("../secret")` returns `HarnessError::WorkspaceEscape`;
  - absolute paths outside the root are rejected;
  - read helpers preserve UTF-8 error reporting.

  Verification:

  ```bash
  cargo test -p codel00p-harness --test workspace_paths
  ```

  Commit:

  ```bash
  git commit -m "feat: enforce harness workspace boundaries"
  ```

- [x] 4. Add tool trait, result shape, and registry

  Write tests for deterministic registration, lookup, dispatch, and unknown
  tool errors.

  Files:

  - `crates/codel00p-harness/src/tools.rs`
  - `crates/codel00p-harness/src/tool_registry.rs`
  - `crates/codel00p-harness/src/tool_result.rs`
  - `crates/codel00p-harness/tests/tool_registry.rs`

  Target trait:

  ```rust
  #[async_trait::async_trait]
  pub trait Tool: Send + Sync {
      fn name(&self) -> &'static str;
      fn description(&self) -> &'static str;
      fn input_schema(&self) -> serde_json::Value;

      async fn execute(
          &self,
          workspace: &Workspace,
          input: serde_json::Value,
      ) -> Result<ToolResult, HarnessError>;
  }
  ```

  Verification:

  ```bash
  cargo test -p codel00p-harness --test tool_registry
  ```

  Commit:

  ```bash
  git commit -m "feat: add harness tool registry"
  ```

- [x] 5. Implement read-only tools

  Add `list_files`, `read_file`, and `search_text` behind
  `ToolRegistry::read_only_defaults()`.

  Files:

  - `crates/codel00p-harness/src/tools.rs`
  - `crates/codel00p-harness/tests/read_only_tools.rs`

  Required behavior:

  - `list_files` returns stable sorted relative paths;
  - `read_file` returns UTF-8 content and rejects escaped paths;
  - `search_text` returns matching file paths, line numbers, and snippets;
  - all tool outputs are JSON values wrapped in `ToolResult`;
  - large outputs are bounded by explicit limits.

  Verification:

  ```bash
  cargo test -p codel00p-harness --test read_only_tools
  ```

  Commit:

  ```bash
  git commit -m "feat: add read-only harness tools"
  ```

- [x] 6. Define model-client boundary and fake test client

  Keep the harness independent from live providers by introducing a small local
  trait that can be implemented by fakes and by the provider adapter.

  Files:

  - `crates/codel00p-harness/src/turn.rs`
  - `crates/codel00p-harness/tests/support/mod.rs`

  Target trait:

  ```rust
  #[async_trait::async_trait]
  pub trait ModelClient: Send + Sync {
      async fn infer(
          &self,
          request: HarnessInferenceRequest,
      ) -> Result<HarnessInferenceResponse, HarnessError>;
  }
  ```

  Test fixtures should support scripted responses:

  - final assistant text;
  - one or more tool calls;
  - provider error.

  Verification:

  ```bash
  cargo test -p codel00p-harness --tests
  ```

  Commit:

  ```bash
  git commit -m "feat: add harness model client boundary"
  ```

- [x] 7. Implement no-tool turn execution

  Write the first harness loop test where a user turn receives a final assistant
  response without tool calls.

  Files:

  - `crates/codel00p-harness/src/agent.rs`
  - `crates/codel00p-harness/src/turn.rs`
  - `crates/codel00p-harness/src/context.rs`
  - `crates/codel00p-harness/tests/agent_turn.rs`

  Required assertions:

  - user message is appended;
  - model request contains session messages;
  - assistant response is appended;
  - outcome has assistant text;
  - events include `TurnStarted`, `ContextBuilt`, `InferenceRequested`,
    `InferenceCompleted`, and `TurnCompleted`.

  Verification:

  ```bash
  cargo test -p codel00p-harness --test agent_turn
  ```

  Commit:

  ```bash
  git commit -m "feat: run basic harness turns"
  ```

- [x] 8. Implement tool-call turn loop

  Extend the loop so model-requested tool calls are executed, tool results are
  appended, and inference continues until final text or budget exhaustion.

  Files:

  - `crates/codel00p-harness/src/turn.rs`
  - `crates/codel00p-harness/tests/agent_tool_loop.rs`

  Required tests:

  - model calls `read_file`, then returns final text;
  - unknown tool creates controlled tool error;
  - failing tool emits `ToolCallFailed`;
  - repeated tool calls stop at iteration limit;
  - events remain deterministic.

  Verification:

  ```bash
  cargo test -p codel00p-harness --test agent_tool_loop
  ```

  Commit:

  ```bash
  git commit -m "feat: execute harness tool loops"
  ```

- [ ] 9. Add `codel00p-providers` adapter

  Implement a harness adapter that maps the local `ModelClient` trait to
  `codel00p-providers::InferenceClient`.

  Files:

  - `crates/codel00p-harness/src/provider_adapter.rs`
  - `crates/codel00p-harness/src/lib.rs`
  - `crates/codel00p-harness/tests/provider_adapter.rs`

  Required behavior:

  - converts session messages into `InferenceRequest` messages;
  - converts tool definitions into provider tool definitions;
  - maps provider text responses into `HarnessInferenceResponse`;
  - maps provider tool calls into harness tool calls;
  - preserves provider, model, finish reason, and usage metadata when present.

  Verification:

  ```bash
  cargo test -p codel00p-harness --test provider_adapter
  ```

  Commit:

  ```bash
  git commit -m "feat: connect harness to providers"
  ```

- [ ] 10. Document and verify the crate

  Update public docs and run full workspace checks.

  Files:

  - `docs/harness.md`
  - `docs/architecture.md`
  - `README.md`
  - `crates/codel00p-harness/README.md`

  Verification:

  ```bash
  cargo fmt --all -- --check
  cargo test --workspace
  cargo clippy --workspace --all-targets -- -D warnings
  git diff --check
  ```

  Commit:

  ```bash
  git commit -m "docs: document harness implementation"
  ```

## Completion Criteria

- `codel00p-harness` builds as a Rust workspace crate.
- All harness tests use deterministic fake model clients.
- Read-only tools cannot escape the configured workspace root.
- The harness can run a no-tool turn and a tool-call loop.
- The provider adapter is present and tested at the mapping boundary.
- Full workspace formatting, tests, clippy, and diff hygiene pass.
- Every implementation stage is committed separately with no coauthors.
