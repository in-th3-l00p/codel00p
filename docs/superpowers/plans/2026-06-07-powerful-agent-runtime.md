# Powerful Agent Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade codel00p from a read-only harness into a protocol-driven agent runtime foundation with permissions, progress, context pressure, durable sessions, and memory-ready lifecycle hooks.

**Architecture:** Add shared contracts to `codel00p-protocol` first, then wire `codel00p-harness` to those contracts. Add `codel00p-session` as an append-only persistence crate so CLI, desktop, cloud, and memory modules can replay the same session/event stream.

**Tech Stack:** Rust 2024 workspace, `serde`, `serde_json`, `async-trait`, `tokio`, `tempfile`, TDD with crate-level integration tests and golden JSON fixtures.

---

### Task 1: Protocol Runtime Contracts

**Files:**
- Modify: `crates/codel00p-protocol/src/lib.rs`
- Modify: `crates/codel00p-protocol/tests/contracts.rs`
- Modify: `crates/codel00p-protocol/tests/golden_json.rs`
- Create: `crates/codel00p-protocol/tests/fixtures/runtime_contracts.json`

- [ ] **Step 1: Write failing tests**

Add tests proving the protocol can serialize runtime error kinds, permission requests/decisions, context pressure, compaction records, tool progress, and session persistence events.

- [ ] **Step 2: Verify red**

Run: `cargo test -p codel00p-protocol`

Expected: compile failure for missing contract types.

- [ ] **Step 3: Implement minimal protocol types**

Add stable serializable structs/enums with constructors and accessors.

- [ ] **Step 4: Verify green**

Run: `cargo test -p codel00p-protocol`

Expected: all protocol tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/codel00p-protocol
git commit -m "feat: add runtime protocol contracts"
```

### Task 2: Harness Permission And Progress Runtime

**Files:**
- Modify: `crates/codel00p-harness/src/agent.rs`
- Modify: `crates/codel00p-harness/src/tools.rs`
- Modify: `crates/codel00p-harness/src/tool_registry.rs`
- Modify: `crates/codel00p-harness/src/errors.rs`
- Modify: `crates/codel00p-harness/src/lib.rs`
- Test: `crates/codel00p-harness/tests/agent_tool_loop.rs`
- Test: `crates/codel00p-harness/tests/protocol_contracts.rs`

- [ ] **Step 1: Write failing tests**

Add tests for allow/deny permission decisions, blocked tool results, tool progress events, and context pressure data flowing into inference requests.

- [ ] **Step 2: Verify red**

Run: `cargo test -p codel00p-harness`

Expected: compile failure for missing policy/runtime APIs.

- [ ] **Step 3: Implement minimal harness runtime**

Add a permission policy trait, default allow policy, tool metadata, progress-aware tool execution, and context pressure fields.

- [ ] **Step 4: Verify green**

Run: `cargo test -p codel00p-harness`

Expected: all harness tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/codel00p-harness
git commit -m "feat: add harness permission and progress runtime"
```

### Task 3: Durable Session Crate

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/codel00p-session/Cargo.toml`
- Create: `crates/codel00p-session/src/lib.rs`
- Create: `crates/codel00p-session/tests/append_log.rs`
- Create: `crates/codel00p-session/README.md`
- Modify: `docs/architecture.md`

- [ ] **Step 1: Write failing tests**

Add tests for creating a session, appending messages/events, parent session lineage, replaying records in order, and rejecting records for the wrong session.

- [ ] **Step 2: Verify red**

Run: `cargo test -p codel00p-session`

Expected: package not found or missing type failures.

- [ ] **Step 3: Implement minimal in-memory append log**

Build a deterministic storage trait and an in-memory implementation. SQLite remains a later backend behind the same trait.

- [ ] **Step 4: Verify green**

Run: `cargo test -p codel00p-session`

Expected: all session crate tests pass.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/codel00p-session docs/architecture.md
git commit -m "feat: add durable session log crate"
```

### Task 4: Lifecycle Hooks And Memory-Ready Contracts

**Files:**
- Modify: `crates/codel00p-harness/src/context.rs`
- Modify: `crates/codel00p-harness/src/agent.rs`
- Test: `crates/codel00p-harness/tests/agent_tool_loop.rs`
- Modify: `docs/harness.md`
- Modify: `docs/memory.md`
- Modify: `docs/research/claude-code-agent-reference.md`

- [ ] **Step 1: Write failing tests**

Add tests proving turn lifecycle hooks run in order and non-fatal hook failures become structured events instead of crashing the turn.

- [ ] **Step 2: Verify red**

Run: `cargo test -p codel00p-harness`

Expected: compile failure for missing lifecycle API.

- [ ] **Step 3: Implement lifecycle hooks**

Add hook traits for turn start, pre-inference, post-tool, pre-compact, and turn complete. Keep memory storage out of harness.

- [ ] **Step 4: Verify green**

Run: `cargo test -p codel00p-harness`

Expected: all harness tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/codel00p-harness docs
git commit -m "feat: add harness lifecycle hooks"
```

### Task 5: Full Workspace Gate

**Files:**
- All modified files.

- [ ] **Step 1: Run format check**

Run: `cargo fmt --all -- --check`

Expected: exit 0.

- [ ] **Step 2: Run full tests**

Run: `cargo test --workspace`

Expected: exit 0.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: exit 0.

- [ ] **Step 4: Run diff whitespace check**

Run: `git diff --check`

Expected: exit 0.
