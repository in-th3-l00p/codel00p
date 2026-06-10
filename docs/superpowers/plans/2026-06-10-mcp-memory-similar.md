# MCP Memory Similar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose active near-duplicate memory scoring through the MCP server.

**Architecture:** Add a `memory_similar` MCP tool with `content`, `kind`,
optional `threshold`, and optional `limit` arguments. It should call
`MemoryRepository::similar_active` and return a JSON array of memory record
objects with `score`.

**Tech Stack:** Rust, `codel00p-cli` MCP server, `codel00p-memory`,
stdio JSON-RPC tests.

---

### Task 1: Add Red MCP Similar Test

**Files:**
- Modify: `core/crates/codel00p-cli/tests/mcp_server_cli.rs`

- [x] **Step 1: Assert MCP tool list and call include `memory_similar`**

Extend the MCP memory tool test to assert `memory_similar` is advertised, then
call it against seeded memory with:

```json
{ "content": "Run pnpm verify before pushing to main branch.", "kind": "workflow", "threshold": 70 }
```

Assert the returned JSON includes `id`, `status`, `kind`, `score`, `content`,
tags, and source evidence.

- [x] **Step 2: Run focused MCP test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-cli --test mcp_server_cli mcp_serve_exposes_memory_tools_over_stdio_json_rpc
```

Expected: FAIL because `memory_similar` is not advertised or callable yet.

### Task 2: Implement MCP Similar Tool

**Files:**
- Modify: `core/crates/codel00p-cli/src/mcp_server.rs`

- [x] **Step 1: Add tool descriptor and dispatch**

Advertise `memory_similar` and dispatch `tools/call` requests to a new handler.

- [x] **Step 2: Serialize scored record JSON**

Parse required `content` and `kind`, optional `threshold` and `limit`, call
`similar_active`, and return JSON records with `score`.

- [x] **Step 3: Run focused MCP test to verify pass**

Run:

```bash
cd core
cargo test -p codel00p-cli --test mcp_server_cli mcp_serve_exposes_memory_tools_over_stdio_json_rpc
```

Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-cli/README.md`
- Modify: `core/crates/codel00p-memory/README.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Document MCP similar scoring**

Mention `memory_similar` in CLI/MCP docs and Memory 2.0 notes.

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
git add docs/superpowers/plans/2026-06-10-mcp-memory-similar.md core/crates/codel00p-cli core/crates/codel00p-memory/README.md docs/agentic-backlog.md docs/product-roadmap.md
git commit -m "feat: add mcp memory similar tool"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
