# MCP Memory List And Search Source Evidence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Include source session and turn evidence in MCP `memory_list` and `memory_search` JSON responses.

**Architecture:** Reuse the MCP `memory_record_json` helper for list output. Add a small retrieved-memory JSON helper that extends `memory_record_json` with retrieval `reason`, so search output keeps its current reason field while gaining the same `source` object as show/resource reads.

**Tech Stack:** Rust, `codel00p-cli` MCP server, stdio JSON-RPC integration tests.

---

### Task 1: Add Red MCP List/Search Source Evidence Test

**Files:**
- Modify: `core/crates/codel00p-cli/tests/mcp_server_cli.rs`

- [x] **Step 1: Assert list and search include source evidence**

In `mcp_serve_can_create_and_review_project_memory`, assert the `memory_search`
response includes source evidence for the approved memory. Add a `memory_list`
call for the approved memory before archive, and assert it includes source
evidence as well.

- [x] **Step 2: Run the focused MCP workflow test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-cli --test mcp_server_cli mcp_serve_can_create_and_review_project_memory
```

Expected: FAIL because `memory_list` and `memory_search` do not include source evidence yet.

### Task 2: Implement Shared Source Evidence JSON

**Files:**
- Modify: `core/crates/codel00p-cli/src/mcp_server.rs`

- [x] **Step 1: Reuse `memory_record_json` for list output**

Change `memory_list` to map records through `memory_record_json`.

- [x] **Step 2: Add retrieved-memory JSON helper**

Add a helper for `RetrievedMemory` that starts from `memory_record_json` and adds
the existing `reason` field. Change `memory_search` to use it.

- [x] **Step 3: Run the focused MCP workflow test to verify pass**

Run:

```bash
cd core
cargo test -p codel00p-cli --test mcp_server_cli mcp_serve_can_create_and_review_project_memory
```

Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-memory/README.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Document list/search source evidence**

Update source evidence docs and Memory 2.0 foundation notes to mention MCP
list/search JSON responses, not only individual memory JSON.

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

- [ ] **Step 4: Commit and push**

Run:

```bash
git add docs/superpowers/plans/2026-06-10-mcp-memory-list-search-source-evidence.md core/crates/codel00p-cli core/crates/codel00p-memory/README.md docs/agentic-backlog.md docs/product-roadmap.md
git commit -m "feat: expose mcp memory list source evidence"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
