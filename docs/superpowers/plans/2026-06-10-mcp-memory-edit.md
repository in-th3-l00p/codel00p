# MCP Memory Edit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `memory_edit` MCP tool so external clients can edit reviewed project memory through the codel00p MCP server.

**Architecture:** Reuse `MemoryRepository::edit` and the existing MCP memory write pattern. The new tool advertises a JSON schema with `id`, `actor`, `content`, and optional `reason`, returns the edited memory JSON, and emits the same memory resource update notification as create/review/archive.

**Tech Stack:** Rust, `codel00p-cli` MCP server, `codel00p-memory`, stdio JSON-RPC integration tests.

---

### Task 1: Add Red MCP Memory Edit Test

**Files:**
- Modify: `core/crates/codel00p-cli/tests/mcp_server_cli.rs`

- [x] **Step 1: Assert the tool is listed**

In `mcp_serve_exposes_memory_tools_over_stdio_json_rpc`, add:

```rust
assert!(tool_names.contains(&"memory_edit"));
```

Place it after the `memory_archive` assertion.

- [x] **Step 2: Add an edit call to the memory workflow test**

In `mcp_serve_can_create_and_review_project_memory`, after the approve call and before the search call, add:

```rust
send(
    &mut child,
    json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "memory_edit",
            "arguments": {
                "id": "mem-mcp-1",
                "actor": "mcp-client",
                "content": "Use MCP edit tools for reviewed project memory.",
                "reason": "clarified editable workflow"
            }
        }
    }),
);
let edited = read_response(&mut stdout);
assert_eq!(edited["result"]["isError"], false);
let edited_text = edited["result"]["content"][0]["text"]
    .as_str()
    .expect("edited text");
assert!(edited_text.contains(r#""status":"approved""#));
assert!(edited_text.contains("Use MCP edit tools for reviewed project memory."));
```

Then update the following search request id from `4` to `5`, change the search text from `MCP write tools` to `MCP edit tools`, and update later request ids in that test by one.

- [x] **Step 3: Run the focused MCP test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-cli --test mcp_server_cli mcp_serve_can_create_and_review_project_memory
```

Expected: FAIL because `memory_edit` is not a known codel00p MCP tool yet.

### Task 2: Implement MCP Memory Edit

**Files:**
- Modify: `core/crates/codel00p-cli/src/mcp_server.rs`

- [x] **Step 1: Import `MemoryEdit`**

Change the memory import to:

```rust
use codel00p_memory::{
    MemoryCandidateInput, MemoryEdit, MemoryListFilter, MemoryQuery, MemoryRepository,
    ReviewDecision,
};
```

- [x] **Step 2: Add the tool descriptor**

In `mcp_tools`, add this descriptor after `memory_archive`:

```rust
json!({
    "name": "memory_edit",
    "description": "Edit one codel00p project memory record.",
    "inputSchema": {
        "type": "object",
        "required": ["id", "actor", "content"],
        "properties": {
            "id": { "type": "string" },
            "actor": { "type": "string" },
            "content": { "type": "string" },
            "reason": { "type": "string" }
        }
    }
}),
```

- [x] **Step 3: Route the tool call**

In `call_tool`, add this match arm after `memory_archive`:

```rust
"memory_edit" => memory_edit(config, &arguments)?,
```

- [x] **Step 4: Add the edit handler**

Add this function after `memory_review`:

```rust
fn memory_edit(config: &CliConfig, arguments: &Value) -> Result<(String, Vec<String>), String> {
    let id = required_string(arguments, "id")?;
    let actor = required_string(arguments, "actor")?;
    let content = required_string(arguments, "content")?;
    let mut edit = MemoryEdit::replace_content(actor, content);
    if let Some(reason) = optional_string(arguments, "reason") {
        edit = edit.with_reason(reason);
    }

    let mut store = open_memory_store(config)?;
    let record = store.edit(id, edit).map_err(|error| error.to_string())?;
    let text =
        serde_json::to_string(&memory_record_json(&record)).map_err(|error| error.to_string())?;
    Ok((text, vec![memory_resource_uri(id)]))
}
```

- [x] **Step 5: Run the focused MCP test to verify pass**

Run:

```bash
cd core
cargo test -p codel00p-cli --test mcp_server_cli mcp_serve_can_create_and_review_project_memory
```

Expected: PASS.

- [x] **Step 6: Run the tool-list focused test**

Run:

```bash
cd core
cargo test -p codel00p-cli --test mcp_server_cli mcp_serve_exposes_memory_tools_over_stdio_json_rpc
```

Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-memory/README.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Document MCP edit availability**

In `core/crates/codel00p-memory/README.md`, update the edit audit section to say the MCP server exposes the same operation as `memory_edit`.

In `docs/product-roadmap.md` and `docs/agentic-backlog.md`, update Memory 2.0 wording from CLI-backed edits to CLI/MCP-backed edits.

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
git add docs/superpowers/plans/2026-06-10-mcp-memory-edit.md core/crates/codel00p-cli core/crates/codel00p-memory/README.md docs/agentic-backlog.md docs/product-roadmap.md
git commit -m "feat: add mcp memory edit tool"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
