# Memory Source URI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a direct source replay URI to memory record output so reviewers can
jump from memory JSON/text to the originating session replay resource.

**Architecture:** Keep storage and protocol structs unchanged. Derive
`source_uri` from `MemorySource.session_id` in CLI/MCP serializers as
`codel00p://sessions/{session_id}`.

**Tech Stack:** Rust, `codel00p-cli`, `codel00p-memory`, stdio MCP tests.

---

### Task 1: Add Red Source URI Tests

**Files:**
- Modify: `core/crates/codel00p-cli/tests/memory_cli.rs`
- Modify: `core/crates/codel00p-cli/tests/mcp_server_cli.rs`

- [x] **Step 1: Assert CLI memory JSON/text includes `source_uri`**

Extend memory show/list JSON coverage and text detail coverage to assert:

```json
"source_uri":"codel00p://sessions/session-1"
```

- [x] **Step 2: Assert MCP memory JSON includes `source_uri`**

Extend MCP memory show/list/search/similar/resource coverage to assert the same
source replay URI is exposed for MCP clients.

- [x] **Step 3: Run focused tests to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_show_and_audit_print_stable_details
cargo test -p codel00p-cli --test mcp_server_cli mcp_serve_can_create_and_review_project_memory
```

Expected: FAIL because `source_uri` is not serialized yet.

### Task 2: Implement Source URI Serialization

**Files:**
- Modify: `core/crates/codel00p-cli/src/memory.rs`
- Modify: `core/crates/codel00p-cli/src/mcp_server.rs`

- [x] **Step 1: Add source URI helper**

Add a small helper that derives `codel00p://sessions/{session_id}` from
`MemorySource`.

- [x] **Step 2: Add `source_uri` to CLI and MCP memory JSON**

Include the derived URI in `memory_entry_json` without changing the existing
`source.session_id` and `source.turn_id` fields.

- [x] **Step 3: Add `source_uri` to CLI text detail**

Include a stable `source_uri: ...` line in `memory show` text output when source
evidence is available.

- [x] **Step 4: Run focused tests to verify pass**

Run the same focused tests from Task 1.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-cli/README.md`
- Modify: `core/crates/codel00p-memory/README.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Document source replay URI**

Mention `source_uri` in CLI/MCP memory JSON and source evidence docs.

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
git add docs/superpowers/plans/2026-06-10-memory-source-uri.md core/crates/codel00p-cli core/crates/codel00p-memory/README.md docs/agentic-backlog.md docs/product-roadmap.md
git commit -m "feat: add memory source replay uri"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
