# Production CLI MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a production-usable initial codel00p product: a local CLI coding agent that can inspect a repository, call configured inference providers, persist sessions, extract candidate project memory, and reuse approved memory in future runs.

**Architecture:** Keep the first product as a hardened vertical slice across the existing Rust crates. `codel00p-cli` owns operator-facing commands, `codel00p-harness` owns agent execution, `codel00p-providers` owns inference, `codel00p-memory` owns reviewable project memory, `codel00p-session` owns durable transcript/event replay, and `codel00p-storage` owns backend-neutral persistence. Cloud, desktop, sync, and organization management remain follow-on products once the core loop is stable.

**Tech Stack:** Rust 2024 workspace, Tokio, SQLite storage, provider-neutral chat-completions inference, deterministic integration tests, root `pnpm verify`, and focused commits per working slice.

---

## Product Boundary

The initial production product is complete when a developer can:

- run `codel00p agent run "Inspect this project"` in a repository;
- select a provider and model through flags and environment credentials;
- use read-only workspace tools safely inside the workspace root;
- persist session messages and harness events to SQLite;
- extract candidate memories from completed turns;
- review, approve, reject, archive, and audit memories with the existing CLI;
- have approved memories injected into later provider requests;
- run deterministic tests without live credentials and optional live smoke tests with credentials.

The initial product does not include:

- Electron app;
- cloud organization management;
- multi-user sync;
- billing;
- remote policy server;
- write/edit tools;
- autonomous shell execution.

Those are next products, not blockers for this MVP.

## Acceptance Checklist

- [ ] `codel00p agent run` supports prompt, workspace, session id, provider, model, max iterations, JSON events, and memory extraction toggles.
- [ ] CLI opens one SQLite-backed storage boundary and wires memory plus session persistence consistently.
- [ ] Provider credentials are read from documented environment variables.
- [ ] CLI output is stable for scripts: assistant text on stdout by default, optional JSON lines for event/session diagnostics.
- [ ] Sessions can be replayed through `codel00p session show`.
- [ ] Candidate memory appears in `codel00p memory list --status candidate` after a run that emits remember directives.
- [ ] Approved memory is loaded into future agent turns.
- [ ] All new behavior has deterministic tests using a fake provider transport or direct harness model double.
- [ ] `pnpm verify` passes.

## Task 1: CLI Agent Run

**Files:**
- Modify: `core/crates/codel00p-cli/Cargo.toml`
- Modify: `core/crates/codel00p-cli/src/main.rs`
- Create: `core/crates/codel00p-cli/tests/agent_cli.rs`

- [ ] Write a failing CLI test that runs `codel00p agent run` against a local mock OpenAI-compatible server and a temporary workspace.
- [ ] Assert the command sends the user prompt, exposes read-only tools, prints final assistant text, and exits successfully.
- [ ] Implement `agent run` parsing with:
  - `--workspace <path>` defaulting to current directory;
  - `--provider <id>`;
  - `--model <id>`;
  - `--session-id <id>` optional;
  - `--max-iterations <n>` defaulting to harness default;
  - `--json-events` optional.
- [ ] Build an `InferenceClient` with `default_registry()` and environment credentials.
- [ ] Build `ProviderModelClient`, `Workspace`, `ToolRegistry::read_only_defaults()`, `MemoryRepositoryProjectMemoryProvider`, `ExplicitTurnMemoryExtractor`, and `MemoryRepositoryCandidateSink`.
- [ ] Run `cargo test -p codel00p-cli --test agent_cli`.
- [ ] Commit: `feat: add agent run cli`

## Task 2: Session Persistence

**Files:**
- Modify: `core/crates/codel00p-cli/src/main.rs`
- Modify: `core/crates/codel00p-cli/tests/agent_cli.rs`
- Modify: `core/crates/codel00p-cli/README.md`

- [ ] Write a failing test proving `agent run --session-id session-cli` stores user, assistant, tool, and event records in SQLite.
- [ ] Persist every `TurnOutcome.session_state().messages()` item and every `TurnOutcome.events()` item through `StorageBackedSessionStore<SqliteStorage>`.
- [ ] Create the session metadata if it does not exist; reuse it if it already exists.
- [ ] Add `codel00p session show <session-id>` that prints persisted records in stable tab-separated form.
- [ ] Run `cargo test -p codel00p-cli`.
- [ ] Commit: `feat: persist cli agent sessions`

## Task 3: Memory Growth Workflow

**Files:**
- Modify: `core/crates/codel00p-cli/src/main.rs`
- Modify: `core/crates/codel00p-cli/tests/agent_cli.rs`
- Modify: `core/crates/codel00p-cli/README.md`

- [ ] Write a failing test where the assistant response includes `remember workflow: Run pnpm verify before pushing main.`
- [ ] Assert `memory list --status candidate` shows the extracted candidate after `agent run`.
- [ ] Approve the candidate and run another agent turn.
- [ ] Assert the provider receives the approved memory as a leading `Project memory:` system message.
- [ ] Run `cargo test -p codel00p-cli`.
- [ ] Commit: `feat: connect cli agent memory loop`

## Task 4: Production Hardening

**Files:**
- Modify: `core/crates/codel00p-cli/src/main.rs`
- Modify: `core/crates/codel00p-cli/tests/agent_cli.rs`
- Modify: `core/crates/codel00p-cli/README.md`
- Modify: `README.md`
- Modify: `docs/roadmap.md`

- [ ] Add friendly error messages for missing provider, missing model, missing credentials, bad workspace, and invalid iteration count.
- [ ] Add `--help` output for top-level, `memory`, `agent`, and `session` commands.
- [ ] Document environment variables for GitHub Copilot, OpenAI, Anthropic, OpenRouter, Azure, Gemini, and custom providers.
- [ ] Document the production MVP and what remains outside the initial release.
- [ ] Run `pnpm verify`.
- [ ] Commit: `docs: document production cli mvp`

## Task 5: Release Gate

**Files:**
- Modify: `README.md`
- Modify: `docs/roadmap.md`
- Modify: `core/crates/codel00p-cli/README.md`

- [ ] Run `pnpm verify`.
- [ ] Run optional live tests only when credentials are available: `CODEL00P_INTEGRATION_TESTS=1 cargo test --workspace -- --ignored`.
- [ ] Confirm `git status --short --branch` is clean.
- [ ] Push `main`.
- [ ] Tag only after the user explicitly asks for a release tag.

