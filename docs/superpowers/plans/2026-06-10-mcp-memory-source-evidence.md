# MCP Memory Source Evidence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Include source session and turn evidence in MCP memory JSON responses.

**Architecture:** Extend the existing `memory_record_json` helper used by MCP `memory_show`, memory write responses, and memory resource reads. When a memory entry has `MemorySource`, add a `source` object with `session_id` and `turn_id`; records without source keep the existing shape.

**Tech Stack:** Rust, `codel00p-cli` MCP server, `codel00p-memory`, stdio JSON-RPC integration tests.

---

### Task 1: Add Red MCP Source Evidence Test

**Files:**
- Modify: `core/crates/codel00p-cli/tests/mcp_server_cli.rs`

- [x] **Step 1: Assert memory resources include source evidence**

In `mcp_serve_exposes_memory_and_session_resources`, after the existing memory text assertions:

```rust
assert!(memory_text.contains(r#""source":{"session_id":"session-resource","turn_id":"turn-resource"}"#));
```

- [x] **Step 2: Run the focused resource test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-cli --test mcp_server_cli mcp_serve_exposes_memory_and_session_resources
```

Expected: FAIL because MCP memory JSON does not include source evidence yet.

### Task 2: Implement MCP Source Evidence JSON

**Files:**
- Modify: `core/crates/codel00p-cli/src/mcp_server.rs`

- [x] **Step 1: Update `memory_record_json`**

Replace `memory_record_json` with:

```rust
fn memory_record_json(record: &codel00p_memory::MemoryRecord) -> Value {
    let mut item = json!({
        "id": record.entry().id(),
        "status": status_label(record.entry().status()),
        "kind": kind_label(record.entry().kind()),
        "content": record.entry().content(),
        "tags": record.entry().tags(),
    });
    if let Some(source) = record.entry().source() {
        item["source"] = json!({
            "session_id": source.session_id().as_str(),
            "turn_id": source.turn_id().as_str(),
        });
    }
    item
}
```

- [x] **Step 2: Run the focused resource test to verify pass**

Run:

```bash
cd core
cargo test -p codel00p-cli --test mcp_server_cli mcp_serve_exposes_memory_and_session_resources
```

Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-memory/README.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Document MCP source evidence**

In `core/crates/codel00p-memory/README.md`, update the source evidence section to mention MCP memory JSON includes `source.session_id` and `source.turn_id` when present.

In `docs/product-roadmap.md`, update the Memory 2.0 source evidence foundation bullet to mention MCP memory JSON.

In `docs/agentic-backlog.md`, update the Memory 2.0 source evidence bullet to mention MCP-visible source session/turn metadata.

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
git add docs/superpowers/plans/2026-06-10-mcp-memory-source-evidence.md core/crates/codel00p-cli core/crates/codel00p-memory/README.md docs/agentic-backlog.md docs/product-roadmap.md
git commit -m "feat: expose mcp memory source evidence"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
