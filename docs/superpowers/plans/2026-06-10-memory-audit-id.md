# Memory Audit ID Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make memory audit JSON events self-describing by including the memory
record id on every event.

**Architecture:** Storage already records `memory_id` on `MemoryAuditEvent`.
Expose that value in CLI `memory audit --json` and MCP `memory_audit` JSON
without changing TSV output or storage schemas.

**Tech Stack:** Rust, `codel00p-cli`, `codel00p-memory`, CLI and stdio MCP
tests.

---

### Task 1: Add Red Audit ID Tests

**Files:**
- Modify: `core/crates/codel00p-cli/tests/memory_cli.rs`
- Modify: `core/crates/codel00p-cli/tests/mcp_server_cli.rs`

- [x] **Step 1: Assert CLI audit JSON includes `memory_id`**

Extend edit/restore audit JSON tests to assert each event includes the memory
id.

- [x] **Step 2: Assert MCP audit JSON includes `memory_id`**

Extend MCP audit assertions to require `"memory_id":"mem-mcp-1"` in audit
tool output.

- [x] **Step 3: Run focused tests to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_edit_updates_content_and_prints_audit_event
cargo test -p codel00p-cli --test mcp_server_cli mcp_serve_can_create_and_review_project_memory
```

Expected: FAIL because audit JSON does not expose `memory_id` yet.

### Task 2: Implement Audit ID Serialization

**Files:**
- Modify: `core/crates/codel00p-cli/src/memory.rs`
- Modify: `core/crates/codel00p-cli/src/mcp_server.rs`

- [x] **Step 1: Add `memory_id` to CLI audit JSON**

Include `event.memory_id()` in the JSON object emitted by `memory audit
--json`.

- [x] **Step 2: Add `memory_id` to MCP audit JSON**

Include `event.memory_id()` in `memory_audit` MCP tool output.

- [x] **Step 3: Run focused tests to verify pass**

Run the same focused tests from Task 1.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-cli/README.md`
- Modify: `core/crates/codel00p-memory/README.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Document audit event memory ids**

Mention that audit JSON events include `memory_id`.

- [x] **Step 2: Run focused verification**

Run:

```bash
cd core
cargo fmt --all
cargo test -p codel00p-cli --test memory_cli
cargo test -p codel00p-cli --test mcp_server_cli
cargo clippy -p codel00p-cli --all-targets -- -D warnings
```

Expected: all commands pass.

- [x] **Step 3: Run repo verification**

Run:

```bash
pnpm verify
```

Expected: PASS.

- [x] **Step 4: Commit and push**

Run:

```bash
git add docs/superpowers/plans/2026-06-10-memory-audit-id.md core/crates/codel00p-cli core/crates/codel00p-memory/README.md docs/agentic-backlog.md docs/product-roadmap.md
git commit -m "feat: include memory id in audit json"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
