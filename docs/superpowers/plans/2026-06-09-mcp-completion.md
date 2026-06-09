# MCP Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring codel00p MCP support to a production-ready baseline comparable to mature coding tools.

**Architecture:** MCP stays behind `codel00p-mcp` and the harness tool registry. Client transports expose tools, resources, prompts, logging, roots, notifications, and reconnecting subscriptions through typed Rust APIs; CLI, desktop, cloud, and session replay consume the same stable harness event shapes.

**Tech Stack:** Rust, Tokio, JSON-RPC 2.0, MCP 2025-06-18, codel00p harness events, cargo integration tests.

---

### Task 1: Protocol Surface

**Files:**
- Modified: `core/crates/codel00p-mcp/src/lib.rs`
- Tested: `core/crates/codel00p-mcp/tests/stdio_process_client.rs`
- Tested: `core/crates/codel00p-mcp/tests/http_client.rs`

- [x] Add prompt descriptors and `prompts/list` / `prompts/get`.
- [x] Add resource templates and `resources/templates/list`.
- [x] Add resource reads with text/blob content support.
- [x] Add `logging/setLevel`.
- [x] Parse logging and prompt-list-change notifications.
- [x] Answer stdio server `roots/list` requests with configured roots.

### Task 2: Long-Lived Subscriptions

**Files:**
- Modified: `core/crates/codel00p-mcp/src/lib.rs`
- Tested: `core/crates/codel00p-mcp/tests/stdio_process_client.rs`

- [x] Add a simple `McpNotificationWorker` for existing client-owned stdio streams.
- [x] Add `McpStdioNotificationSupervisor` for production stdio subscriptions.
- [x] Add bounded reconnect policy.
- [x] Resubscribe resources after process restart.
- [x] Emit connected, subscribed, notification, and reconnecting events.

### Task 3: Product Event Routing

**Files:**
- Modified: `core/crates/codel00p-mcp/src/lib.rs`
- Tested: `core/crates/codel00p-mcp/tests/client_contract.rs`

- [x] Convert subscription events into stable `AgentEvent::ToolProgress`.
- [x] Share MCP notification progress phases between tool calls and background subscriptions.
- [x] Preserve CLI/desktop/cloud/session replay compatibility through protocol events.

### Task 4: Operator Diagnostics

**Files:**
- Modified: `core/crates/codel00p-cli/src/agent.rs`
- Modified: `core/crates/codel00p-cli/src/help.rs`
- Tested: `core/crates/codel00p-cli/tests/agent_cli.rs`
- Tested: `core/crates/codel00p-cli/tests/help_cli.rs`

- [x] Add `agent mcp doctor`.
- [x] Validate configured stdio/HTTP servers.
- [x] Count advertised tools/resources/prompts.
- [x] Redact env/header/bearer-token details in diagnostic output.
- [x] Add CLI help coverage.

### Task 5: Documentation and Verification

**Files:**
- Modified: `core/crates/codel00p-mcp/README.md`
- Modified: `docs/agent-capability-parity.md`
- Modified: `docs/architecture.md`
- Modified: `docs/roadmap.md`

- [x] Document completed MCP client/server surfaces.
- [x] Narrow remaining work to real-world compatibility matrix maintenance.
- [x] Run full repository verification.
- [x] Push completed commits to `main`.

### Task 6: Cursor Pagination and Compatibility Matrix

**Files:**
- Modified: `core/crates/codel00p-mcp/src/lib.rs`
- Modified: `core/crates/codel00p-mcp/tests/stdio_process_client.rs`
- Modified: `core/crates/codel00p-mcp/tests/http_client.rs`
- Created: `docs/mcp-compatibility.md`
- Modified: `README.md`
- Modified: `core/crates/codel00p-mcp/README.md`
- Modified: `docs/agent-capability-parity.md`
- Modified: `docs/architecture.md`
- Modified: `docs/roadmap.md`

- [x] Follow `nextCursor` pagination for stdio and HTTP `tools/list`.
- [x] Follow `nextCursor` pagination for stdio and HTTP `resources/list`.
- [x] Follow `nextCursor` pagination for stdio and HTTP `resources/templates/list`.
- [x] Follow `nextCursor` pagination for stdio and HTTP `prompts/list`.
- [x] Add regression coverage for paginated stdio list methods.
- [x] Add regression coverage for paginated HTTP tool discovery.
- [x] Document the supported MCP compatibility surface and fixture coverage.
- [x] Document third-party certification targets and operational rules.
