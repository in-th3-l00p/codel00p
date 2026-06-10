# MCP Memory Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a read-only `memory_audit` MCP tool so MCP clients can inspect memory lifecycle and edit history.

**Architecture:** Reuse `MemoryRepository::audit_log` from `codel00p-memory`. The MCP tool takes a memory `id` and returns a JSON array of audit events with stable `sequence`, `action`, `actor`, and optional `reason` fields; it does not emit resource update notifications because it is read-only.

**Tech Stack:** Rust, `codel00p-cli` MCP server, `codel00p-memory`, stdio JSON-RPC integration tests.

---

### Task 1: Add Red MCP Memory Audit Test

**Files:**
- Modify: `core/crates/codel00p-cli/tests/mcp_server_cli.rs`

- [x] **Step 1: Assert the tool is listed**

In `mcp_serve_exposes_memory_tools_over_stdio_json_rpc`, add:

```rust
assert!(tool_names.contains(&"memory_audit"));
```

Place it after the `memory_edit` assertion.

- [x] **Step 2: Add an audit call to the memory workflow test**

In `mcp_serve_can_create_and_review_project_memory`, after the `memory_edit` call and before the search call, add:

```rust
send(
    &mut child,
    json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "memory_audit",
            "arguments": {
                "id": "mem-mcp-1"
            }
        }
    }),
);
let audit = read_response(&mut stdout);
assert_eq!(audit["result"]["isError"], false);
let audit_text = audit["result"]["content"][0]["text"]
    .as_str()
    .expect("audit text");
assert!(audit_text.contains(r#""sequence":3"#));
assert!(audit_text.contains(r#""action":"edited""#));
assert!(audit_text.contains(r#""actor":"mcp-client""#));
assert!(audit_text.contains(r#""reason":"clarified editable workflow""#));
```

Then update the following search request id from `5` to `6`, and update later request ids in that test by one.

- [x] **Step 3: Run the focused MCP test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-cli --test mcp_server_cli mcp_serve_can_create_and_review_project_memory
```

Expected: FAIL because `memory_audit` is not a known codel00p MCP tool yet.

### Task 2: Implement MCP Memory Audit

**Files:**
- Modify: `core/crates/codel00p-cli/src/mcp_server.rs`

- [x] **Step 1: Import `MemoryAuditAction`**

Change the memory import to:

```rust
use codel00p_memory::{
    MemoryAuditAction, MemoryCandidateInput, MemoryEdit, MemoryListFilter, MemoryQuery,
    MemoryRepository, ReviewDecision,
};
```

- [x] **Step 2: Add the tool descriptor**

In `mcp_tools`, add this descriptor after `memory_show`:

```rust
json!({
    "name": "memory_audit",
    "description": "Show audit history for one codel00p memory record.",
    "inputSchema": {
        "type": "object",
        "required": ["id"],
        "properties": {
            "id": { "type": "string" }
        }
    }
}),
```

- [x] **Step 3: Route the tool call**

In `call_tool`, add this match arm after `memory_show`:

```rust
"memory_audit" => (memory_audit(config, &arguments)?, Vec::new()),
```

- [x] **Step 4: Add the audit handler**

Add this function after `memory_show`:

```rust
fn memory_audit(config: &CliConfig, arguments: &Value) -> Result<String, String> {
    let id = required_string(arguments, "id")?;
    let store = open_memory_store(config)?;
    let events = store.audit_log(id).map_err(|error| error.to_string())?;
    let items = events
        .iter()
        .map(|event| {
            let mut item = json!({
                "sequence": event.sequence(),
                "action": audit_action_label(event.action()),
                "actor": event.actor(),
            });
            if let Some(reason) = event.reason() {
                item["reason"] = json!(reason);
            }
            item
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&items).map_err(|error| error.to_string())
}
```

- [x] **Step 5: Add the audit action label helper**

Add this helper near `status_label`:

```rust
fn audit_action_label(action: MemoryAuditAction) -> &'static str {
    match action {
        MemoryAuditAction::CandidateCreated => "candidate_created",
        MemoryAuditAction::Approved => "approved",
        MemoryAuditAction::Rejected => "rejected",
        MemoryAuditAction::Archived => "archived",
        MemoryAuditAction::Edited => "edited",
    }
}
```

- [x] **Step 6: Run the focused MCP test to verify pass**

Run:

```bash
cd core
cargo test -p codel00p-cli --test mcp_server_cli mcp_serve_can_create_and_review_project_memory
```

Expected: PASS.

- [x] **Step 7: Run the tool-list focused test**

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

- [x] **Step 1: Document MCP audit availability**

In `core/crates/codel00p-memory/README.md`, mention the MCP server exposes `memory_audit` for audit-history inspection.

In `docs/product-roadmap.md`, update the Memory 2.0 current foundation audit-history bullet to mention CLI/MCP inspection.

In `docs/agentic-backlog.md`, update the Memory 2.0 edit/audit wording to mention MCP-visible audit events.

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
git add docs/superpowers/plans/2026-06-10-mcp-memory-audit.md core/crates/codel00p-cli core/crates/codel00p-memory/README.md docs/agentic-backlog.md docs/product-roadmap.md
git commit -m "feat: add mcp memory audit tool"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
