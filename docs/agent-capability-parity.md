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
- read-only harness with workspace-safe `list_files`, `read_file`, and
  `search_text` tools;
- typed permission policy and lifecycle hook surfaces;
- deterministic protocol events;
- durable session metadata and append-only replay;
- CLI agent runs through Chat-Completions-compatible providers;
- memory candidate extraction from completed turns;
- human review for candidate memory;
- approved project memory retrieval and injection into later agent turns;
- backend-neutral storage contracts with SQLite-backed persistence.

This is a real foundation, but it is still a read-only memory-aware assistant,
not yet a production-class coding agent.

## Required Parity Track

### 1. Session Continuity

The agent must resume a specific session and continue with prior user,
assistant, and tool messages in the next model request. Session replay should be
the source of truth, and resumed turns must append only new messages and events.

CLI target:

```bash
codel00p agent resume <session-id> "Continue the implementation."
```

### 2. Project Instructions

The runtime must discover and apply hierarchical project instructions:

- repository-level `CODEL00P.md`;
- compatibility imports from `AGENTS.md` and `CLAUDE.md`;
- user-level preferences;
- explicit precedence rules for nested directories.

Instruction loading must be visible in events and testable without network
access.

### 3. Editing Tools

The harness needs write-capable tools before it can implement features:

- apply patch;
- create file;
- update file;
- delete file;
- structured diff summary;
- rollback metadata for failed turns.

File mutation must require permission by default and remain scoped to approved
workspace roots.

### 4. Shell Execution

The agent needs command execution for tests, builds, formatters, code search,
package managers, and project scripts. The first version should support:

- explicit command allow/deny policies;
- working-directory restrictions;
- timeout and output limits;
- evented stdout/stderr summaries;
- durable command audit records.

### 5. Git Workflow

The agent should understand and operate on git state:

- inspect status, diff, and log;
- protect unrelated user changes;
- create small commits without coauthor trailers;
- prepare PR-ready summaries;
- avoid destructive operations unless explicitly requested.

### 6. MCP

codel00p should be both an MCP client and, later, an MCP server:

- MCP client for issue trackers, design tools, observability, databases, and
  internal systems;
- MCP server exposing codel00p memory, session, and agent controls to other
  tools;
- permission prompts for external tool calls.

### 7. Context Management

Long-running work needs context pressure handling:

- token budgets;
- compaction hooks;
- session summaries;
- memory promotion recommendations;
- deterministic context manifests for debugging.

### 8. Interactive and Scriptable Interfaces

The CLI should support:

- one-shot agent runs;
- session resume;
- most-recent session continue;
- JSON event output;
- streaming output;
- explicit plan mode;
- permission mode selection.

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
