# MCP Memory Restore Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose CLI memory restore semantics through the MCP server.

**Architecture:** Add a `memory_restore` MCP tool with `id`, `sequence`,
`actor`, and optional `reason` arguments. It should look up the requested audit
event, restore its `previous_content` through `MemoryRepository::edit`, and
return the same JSON record shape as `memory_edit`.

**Tech Stack:** Rust, `codel00p-cli` MCP server, `codel00p-memory`, stdio
JSON-RPC tests.

---

### Task 1: Add Red MCP Restore Test

**Files:**
- Modify: `core/crates/codel00p-cli/tests/mcp_server_cli.rs`

- [x] **Step 1: Assert tool list includes `memory_restore`**

Extend the MCP tool list test to assert `memory_restore` is advertised.

- [x] **Step 2: Assert MCP restore reverts edited content**

Extend the MCP memory workflow test after `memory_edit` to call:

```json
{ "id": "mem-mcp-1", "sequence": 3, "actor": "mcp-client", "reason": "undo edit" }
```

Assert the response restores the previous content and a later `memory_audit`
call includes the restore edit event.

- [x] **Step 3: Run focused MCP test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-cli --test mcp_server_cli mcp_serve_exposes_memory_tools_over_stdio_json_rpc
cargo test -p codel00p-cli --test mcp_server_cli mcp_serve_can_create_and_review_project_memory
```

Expected: FAIL because `memory_restore` is not advertised or callable yet.

### Task 2: Implement MCP Restore Tool

**Files:**
- Modify: `core/crates/codel00p-cli/src/mcp_server.rs`

- [x] **Step 1: Add tool descriptor and dispatch**

Advertise `memory_restore` and dispatch `tools/call` requests to a new handler.

- [x] **Step 2: Restore content from edit audit sequence**

Parse `id`, `sequence`, `actor`, and optional `reason`; find
`previous_content` in the audit log; call `MemoryRepository::edit`.

- [x] **Step 3: Emit updated memory resource notification**

Return the updated memory record JSON and the `codel00p://memory/{id}` updated
resource URI.

- [x] **Step 4: Run focused MCP tests to verify pass**

Run the same focused tests from Task 1.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-cli/README.md`
- Modify: `core/crates/codel00p-memory/README.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Document MCP restore parity**

Mention `memory_restore` in MCP docs and Memory 2.0 revision notes.

- [x] **Step 2: Run focused verification**

Run:

```bash
cd core
cargo fmt --all
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
git add docs/superpowers/plans/2026-06-10-mcp-memory-restore.md core/crates/codel00p-cli core/crates/codel00p-memory/README.md docs/agentic-backlog.md docs/product-roadmap.md
git commit -m "feat: add mcp memory restore tool"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
