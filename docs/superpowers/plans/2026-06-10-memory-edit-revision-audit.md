# Memory Edit Revision Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve machine-readable edit revision details in memory audit history.

**Architecture:** Extend `MemoryAuditEvent` with optional `previous_content` and `new_content` fields. Only edit events populate these fields. The repository should capture the current content before replacing it. MCP `memory_audit` JSON should include the fields when present. The existing CLI audit TSV remains unchanged in this slice to avoid mixing raw content into tabular output.

**Tech Stack:** Rust, `codel00p-memory`, `codel00p-cli` MCP server, serde JSON audit serialization.

---

### Task 1: Add Red Revision Audit Tests

**Files:**
- Modify: `core/crates/codel00p-memory/tests/lifecycle.rs`
- Modify: `core/crates/codel00p-cli/tests/mcp_server_cli.rs`

- [x] **Step 1: Assert core edit audit serializes content revisions**

In `memory_edit_updates_content_and_audits_revision`, serialize the edit audit
event and assert `previous_content` and `new_content` match the content before
and after the edit.

- [x] **Step 2: Assert MCP memory_audit exposes revision fields**

In `mcp_serve_can_create_and_review_project_memory`, assert the `memory_audit`
tool output contains `previous_content` and `new_content` for the edit event.

- [x] **Step 3: Run focused tests to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-memory --test lifecycle memory_edit_updates_content_and_audits_revision
cargo test -p codel00p-cli --test mcp_server_cli mcp_serve_can_create_and_review_project_memory
```

Expected: FAIL because edit audit events do not store revision content yet.

### Task 2: Implement Revision Audit Metadata

**Files:**
- Modify: `core/crates/codel00p-memory/src/lib.rs`
- Modify: `core/crates/codel00p-cli/src/mcp_server.rs`

- [x] **Step 1: Add optional revision fields to `MemoryAuditEvent`**

Add `previous_content` and `new_content` optional fields with serde skip when
absent, plus public accessors.

- [x] **Step 2: Capture edit content before and after replacement**

Change the edit audit constructor to accept the prior and replacement content,
and pass both values from `StorageBackedMemoryStore::edit`.

- [x] **Step 3: Expose revision fields through MCP audit JSON**

Update `memory_audit` response construction to include `previous_content` and
`new_content` when present.

- [x] **Step 4: Run focused tests to verify pass**

Run:

```bash
cd core
cargo test -p codel00p-memory --test lifecycle memory_edit_updates_content_and_audits_revision
cargo test -p codel00p-cli --test mcp_server_cli mcp_serve_can_create_and_review_project_memory
```

Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-memory/README.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Document revision audit metadata**

Update the edit audit contract and Memory 2.0 foundation notes to mention
machine-readable previous/new content metadata for edit audit events.

- [x] **Step 2: Run focused verification**

Run:

```bash
cd core
cargo fmt --all
cargo test -p codel00p-memory --test lifecycle
cargo test -p codel00p-cli --test mcp_server_cli
cargo clippy -p codel00p-memory --all-targets -- -D warnings
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
git add docs/superpowers/plans/2026-06-10-memory-edit-revision-audit.md core/crates/codel00p-memory core/crates/codel00p-cli docs/agentic-backlog.md docs/product-roadmap.md
git commit -m "feat: record memory edit revisions"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
