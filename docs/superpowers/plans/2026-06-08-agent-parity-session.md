# Agent Parity Session Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn codel00p from a memory-aware read-only assistant into a production coding agent that can understand repository instructions, edit safely, run verification, manage git, and preserve useful project knowledge.

**Architecture:** Build parity as vertical slices through the Rust harness, protocol, CLI, and docs. Each slice must add one user-visible capability, typed tests, permission boundaries, and durable events before the next riskier capability starts.

**Tech Stack:** Rust workspace under `core/`, `codel00p-harness`, `codel00p-cli`, `codel00p-protocol`, `codel00p-storage`, SQLite feature tests, pnpm repo verification.

---

## Session Strategy

The order matters. The agent should learn the repository's rules before it can
edit files or run commands.

1. Project instructions: load `CODEL00P.md`, `AGENTS.md`, and `CLAUDE.md` into
   every model request with deterministic precedence.
2. Permissioned editing: add patch/file mutation tools behind explicit
   permission scopes and workspace boundaries.
3. Shell execution: run tests, builds, linters, and project commands with
   timeouts, output caps, and audit events.
4. Git workflow: inspect status/diff/log, protect unrelated changes, and create
   clean commits.
5. Streaming CLI events: stream assistant output and harness events for
   interactive use.
6. Context management: summarize sessions, track token pressure, and recommend
   memory promotion.
7. MCP client: connect to external team tools through permissioned tool calls.
8. Provider completeness: implement non-Chat-Completions transports for OpenAI
   Responses, Anthropic, Gemini, Bedrock, and Azure-native routes.
9. Desktop and cloud: consume the same protocol events for team memory review,
   governance, and session supervision.

## Quality Bar

Every slice must:

- start with a failing test;
- pass focused tests before broad verification;
- keep commits small and linear;
- avoid coauthor trailers;
- update docs where behavior changes;
- pass `pnpm verify` before pushing.

## Slice 1: Project Instructions

**Files:**

- Create: `core/crates/codel00p-harness/src/instructions.rs`
- Modify: `core/crates/codel00p-harness/src/lib.rs`
- Modify: `core/crates/codel00p-harness/src/agent.rs`
- Modify: `core/crates/codel00p-harness/src/turn.rs`
- Modify: `core/crates/codel00p-harness/src/provider_adapter.rs`
- Test: `core/crates/codel00p-harness/tests/project_instructions.rs`
- Test: `core/crates/codel00p-harness/tests/provider_adapter.rs`
- Docs: `docs/harness.md`, `core/crates/codel00p-harness/README.md`

Steps:

- [ ] Write loader tests for root `CODEL00P.md`, `AGENTS.md`, and `CLAUDE.md`.
- [ ] Verify the tests fail because the instruction loader does not exist.
- [ ] Add a focused instruction loader with deterministic file order.
- [ ] Add `ProjectInstructions` to `HarnessInferenceRequest`.
- [ ] Inject loaded instructions into each agent request before inference.
- [ ] Add provider-adapter tests proving instructions are the first system
      message, before project memory and user messages.
- [ ] Update docs for instruction files and precedence.
- [ ] Run focused harness tests and `pnpm verify`.
- [ ] Commit as `feat: load project instructions`.

## Slice 2: Permissioned Editing

**Files:**

- Create: `core/crates/codel00p-harness/src/editing.rs`
- Modify: `core/crates/codel00p-harness/src/tools.rs`
- Modify: `core/crates/codel00p-harness/src/tool_registry.rs`
- Modify: `core/crates/codel00p-harness/src/permissions.rs`
- Test: `core/crates/codel00p-harness/tests/editing_tools.rs`

Steps:

- [ ] Test `apply_patch` rejects workspace escapes and malformed patches.
- [ ] Test file mutations require write permission.
- [ ] Implement patch application with deterministic diff output.
- [ ] Add create/update/delete tools with audit-friendly results.
- [ ] Run focused harness tests and `pnpm verify`.
- [ ] Commit as `feat: add permissioned editing tools`.

## Slice 3: Shell Execution

**Files:**

- Create: `core/crates/codel00p-harness/src/commands.rs`
- Modify: `core/crates/codel00p-harness/src/tools.rs`
- Test: `core/crates/codel00p-harness/tests/command_tool.rs`

Steps:

- [ ] Test command working directory is constrained to the workspace.
- [ ] Test timeout, stdout cap, stderr cap, and exit status reporting.
- [ ] Test command tool requires execute permission.
- [ ] Implement command execution with structured results.
- [ ] Run focused harness tests and `pnpm verify`.
- [ ] Commit as `feat: add permissioned command execution`.

## Slice 4: Git Workflow

**Files:**

- Create: `core/crates/codel00p-harness/src/git.rs`
- Test: `core/crates/codel00p-harness/tests/git_workflow.rs`
- Modify: `core/crates/codel00p-cli/src/agent.rs`

Steps:

- [ ] Test status/diff/log tools produce stable structured summaries.
- [ ] Test commit creation rejects coauthor trailers.
- [ ] Test dirty unrelated changes are surfaced before mutation.
- [ ] Implement read git tools first, then guarded commit.
- [ ] Run focused tests and `pnpm verify`.
- [ ] Commit as `feat: add guarded git workflow tools`.

## Slice 5: Remaining Parity Tracks

The remaining tracks should be implemented only after editing, shell, and git
are stable:

- streaming event output for CLI and future desktop/cloud clients;
- context compaction and session summary persistence;
- MCP client support;
- native provider transports;
- desktop and cloud event consumption.
