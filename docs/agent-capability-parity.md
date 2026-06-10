# Agent Capability Parity

codel00p should compete with mature coding agents while staying centered on
reviewed project knowledge and team memory. The goal is not feature mimicry. The
goal is a capable coding agent that can safely work in real repositories, retain
useful knowledge, and expose enough control for teams to trust it.

## Reference Capabilities

Claude Code and Codex establish the current baseline for serious coding-agent
tools:

- understand and edit files across a repository;
- answer architecture and implementation questions from source context;
- run tests, linters, build commands, and fix the resulting failures;
- inspect git history, review diffs, resolve conflicts, commit, and prepare PRs;
- resume and continue conversations by session;
- apply project instructions from files such as `CLAUDE.md` or `AGENTS.md`;
- expose permission policies for reads, writes, shell commands, and external
  tools;
- connect to external tools through MCP;
- stream structured turn events for UIs and automation;
- compact or summarize long context;
- provide non-interactive modes that are scriptable in CI and team workflows.

codel00p must reach that baseline before it can claim to be a complete coding
agent. Its advantage should be the memory layer: every meaningful project fact,
workflow, convention, and decision can become reviewed, reusable team knowledge.

## Current codel00p Capabilities

Implemented today:

- provider-neutral Rust inference crate for multiple provider strategies;
- workspace-safe harness with read-only, editing, shell command, git, and MCP
  tool surfaces;
- typed permission policy and lifecycle hook surfaces;
- deterministic protocol events;
- durable session metadata and append-only replay;
- provider layer routes through OpenAI Responses, Anthropic Messages, AWS
  Bedrock Converse, and Chat-Completions-compatible providers;
- CLI agent runs currently enable Chat-Completions-compatible providers while
  native provider modes remain gated at the CLI boundary;
- session resume through persisted replay;
- project instructions from `CODEL00P.md`, `AGENTS.md`, and `CLAUDE.md`;
- streamable harness events for CLI and future UI consumers;
- context compaction primitives and session summary insertion;
- memory candidate extraction from completed turns;
- human review for candidate memory;
- approved project memory retrieval and injection into later agent turns;
- backend-neutral storage contracts with SQLite-backed persistence;
- MCP client/server runtime with diagnostics, subscriptions, reconnects,
  resource/prompt support, pagination, remembered connector permissions, and
  harness event routing.

This is now a real local coding-agent foundation. It still needs polish and
hardening before it can claim Claude Code-grade product maturity: richer patch
handling, cancellation, background command monitoring, web tools, worktree
isolation, PR workflows, broader providers, stronger memory quality, live MCP
certification, and desktop/cloud control surfaces.

## Required Parity Track

### 1. Session Continuity

Implemented baseline: the agent can resume a specific persisted session and
continue with prior user, assistant, and tool messages in the next model
request. Session replay is the source of truth, and resumed turns append new
messages and events.

Current CLI:

```bash
codel00p agent resume <session-id> "Continue the implementation."
```

Remaining work:

- most-recent-session continue;
- fork/branch session flows;
- better resumed-session summaries for long-running work.

### 2. Project Instructions

Implemented baseline: the runtime discovers and applies root-level project
instructions:

- repository-level `CODEL00P.md`;
- compatibility imports from `AGENTS.md` and `CLAUDE.md`;

Remaining work:

- user-level preferences;
- explicit precedence rules for nested directories.

Instruction loading must be visible in events and testable without network
access.

### 3. Editing Tools

Implemented baseline:

- apply patch;
- create file;
- update file;
- delete file;

Remaining work:

- structured diff summary;
- rollback metadata for failed turns.
- richer patch/diff format beyond exact replacements.

File mutation must require permission by default and remain scoped to approved
workspace roots.

### 4. Shell Execution

Implemented baseline: the agent can run structured commands inside the
workspace with permission checks, working-directory restrictions, timeout, and
output limits.

Remaining work:

- explicit command allow/deny policies;
- evented stdout/stderr summaries;
- durable command audit records.
- background command monitoring.

### 5. Git Workflow

Implemented baseline:

- inspect status, diff, and log;
- create small commits without coauthor trailers;

Remaining work:

- protect unrelated user changes across richer edit flows;
- resolve conflicts;
- prepare PR-ready summaries;
- avoid destructive operations unless explicitly requested.

### 6. MCP

codel00p should be both an MCP client and, later, an MCP server:

- MCP client for issue trackers, design tools, observability, databases, and
  internal systems;
- MCP server exposing codel00p memory, session, and agent controls to other
  tools;
- permission prompts for external tool calls.

Current implementation:

- `codel00p-mcp` defines transport-neutral MCP tool/resource descriptors;
- MCP tools can be adapted into `codel00p-harness` tools with stable
  `mcp.<server>.<tool>` names;
- discovered MCP tools default to `PermissionScope::ExternalConnector` and can
  be registered into a `ToolRegistry`;
- stdio MCP servers can be launched, initialized, listed, called, timed out,
  and shut down by the runtime;
- HTTP MCP endpoints can be called with JSON or SSE responses, optional bearer
  tokens, static headers, and `Mcp-Session-Id` reuse;
- `agent run --mcp-server` can attach ad hoc stdio servers with argv/env;
- `.codel00p/mcp.json` can define workspace MCP servers, request timeouts, and
  server/tool permission scopes;
- `agent mcp list` inspects configured tools without making a model call;
- `agent mcp doctor` validates configured stdio/HTTP servers, counts advertised
  tools/resources/prompts, and prints redacted diagnostics without leaking env
  values or bearer tokens;
- MCP permission requests and denials flow through the same JSON event stream
  and session audit log as native tools;
- `codel00p mcp serve` exposes memory search/list/show, reviewed memory
  create/approve/reject/archive tools, and read-only session replay to other
  MCP clients over stdio;
- `codel00p-mcp` provides reusable server runtime helpers for JSON-RPC
  response/error framing, MCP progress notifications, resource subscriptions,
  and resource update notifications;
- reusable typed MCP server handler traits and stdio server runner wiring let
  future codel00p MCP servers be assembled without CLI-specific protocol loops;
- stdio and HTTP MCP clients collect server progress/resource notifications
  and surface them as structured harness `ToolProgress` events;
- stdio and HTTP MCP clients support resource templates, resource reads,
  prompt discovery/materialization, cursor pagination for list endpoints, and
  `logging/setLevel`;
- stdio clients answer server-originated `roots/list` requests with configured
  client roots;
- stdio MCP clients can subscribe/unsubscribe resource URIs and poll later
  resource-updated, tools-list-changed, resources-list-changed,
  prompts-list-changed, and logging-message
  notifications from the long-lived server process;
- `McpNotificationWorker` can run the stdio subscription read loop in the
  background and stream notifications or subscription errors over a channel;
- `McpStdioNotificationSupervisor` initializes stdio MCP subscriptions,
  reconnects crashed servers with bounded backoff, resubscribes resources, and
  converts subscription events into stable harness `ToolProgress` events for
  UI and session replay consumers;
- the stdio MCP server exposes `codel00p://memory/{id}` and
  `codel00p://sessions/{session_id}` JSON resource templates for direct
  context browsing;
- resource clients can subscribe to memory URIs and receive
  `notifications/resources/updated` after memory create/review mutations;
- `tools/call` and `resources/read` requests with `_meta.progressToken`
  receive `notifications/progress` before the final response;
- `--remember-permissions` persists ask-mode MCP connector allow/deny decisions
  per project, tool, and permission scope;
- `codel00p mcp permissions list` and `forget` inspect and revoke remembered
  connector decisions with scriptable TSV output.

Remaining hardening:

- maintain a compatibility matrix against popular third-party MCP servers and
  add regression fixtures for edge cases found there. The baseline matrix lives
  in [MCP Compatibility](mcp-compatibility.md);

### 7. Context Management

Long-running work needs context pressure handling:

Implemented baseline:

- context pressure state on inference requests;
- compaction hooks;
- session-summary insertion;

Remaining work:

- token budgets;
- memory promotion recommendations;
- deterministic context manifests for debugging.

### 8. Interactive and Scriptable Interfaces

The CLI should support:

- one-shot agent runs;
- session resume;
- JSON event output;
- streaming output;
- permission mode selection;

Remaining work:

- most-recent session continue;
- explicit plan mode;
- richer setup/init flows.

The desktop and cloud products should consume the same protocol events instead
of inventing separate runtime contracts.

### 9. Subagents and Long-Horizon Work

The final product should coordinate work across multiple focused agents:

- independent research agents;
- implementation agents;
- review agents;
- memory extraction agents;
- resumable long-running goals.

Subagents must share reviewed memory and session events, not hidden ad hoc
state.

## Product Standard

codel00p becomes production-worthy when a team can point it at a real
repository, give it a task, let it inspect/edit/test/commit safely, and trust
that the important learnings are captured as reviewed project memory for the
next teammate or future agent session.
