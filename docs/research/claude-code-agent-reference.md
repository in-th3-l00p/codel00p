# Claude Code Agent Reference Notes

These notes summarize architecture ideas observed from
`yasasbanukaofficial/claude-code` at commit
`a371abbe75ffa0d0a3c92290e2bbf56a7ef54367`.

The referenced repository describes itself as leaked Anthropic source and does
not ship an open-source license. Treat it strictly as product and architecture
research. Do not copy implementation code, file structure, prompts, or private
constants into codel00p.

## Relevant Lessons

### Agent Lifecycle

A production coding agent needs a durable turn lifecycle:

- build session context from the current transcript, workspace, memory, skills,
  policy, and runtime state;
- persist the user's turn before inference;
- stream model and tool progress as typed events;
- record assistant tool-call declarations before tool results;
- append tool results back into the transcript in provider-compatible order;
- continue until final assistant text, cancellation, or an iteration limit.

codel00p already models this through `codel00p-harness`, `codel00p-providers`,
and `codel00p-protocol`. The next step is to make persistence and replay first
class instead of keeping turn state only in memory.

### Tool Orchestration

Tool execution should be ordered by safety, not by a single global serial loop.
Consecutive calls that are proven read-only and concurrency-safe can execute in
parallel. Unsafe, unknown, or mutating tools must execute serially and act as
batch boundaries.

This is now part of `codel00p-harness`: default read-only tools opt in to
concurrency-safe execution, custom tools are serial by default, and session
messages preserve the model's original tool-call order.

### Memory And Context Pressure

The strongest product lesson is that memory cannot be a passive notes feature.
It needs active lifecycle management:

- extract durable project knowledge from completed work;
- avoid extraction during fragile tool sequences;
- track context-window pressure continuously;
- compact transcript state before the model becomes unreliable;
- suppress repeated warnings and use circuit breakers after failed compaction;
- preserve user intent, pending tasks, tool state, and decisions during compacts.

For codel00p this belongs in a dedicated `codel00p-memory` crate with protocol
contracts for memory entries, compaction events, and review status. The harness
should expose hooks; it should not own memory storage.

### Permissions And Policy

Tool permission checks need to be explicit runtime decisions, not scattered
inside individual tools. A useful policy layer should support:

- workspace trust decisions;
- team or organization rules;
- tool-level allow, deny, and ask modes;
- denial events that become part of the transcript;
- separate handling for read-only, mutating, network, and shell tools.

codel00p should keep read-only tools as the default first milestone and add
mutating tools only after approval contracts exist in `codel00p-protocol`.

### Errors, Progress, And Telemetry

Agent failures should be classified in a way the interface can act on:

- model/provider errors;
- invalid tool input;
- permission denied;
- tool execution failure;
- cancellation;
- context limit pressure;
- iteration limit reached.

Events should be structured enough for CLI rendering, desktop timelines, cloud
audit logs, and test assertions to use the same contract.

## Implementation Backlog

1. Persist and replay `SessionState` with append-only turn records.
2. Add context-pressure accounting and compact-warning protocol events.
3. Design `codel00p-memory` extraction hooks and reviewed memory entries.
4. Add permission-decision contracts before any mutating tool ships.
5. Classify harness/tool/provider errors into stable protocol error kinds.
6. Add progress events for long-running tools.
7. Add cancellation tokens to inference and tool execution.
8. Add MCP/LSP integration behind protocol-safe connector boundaries.
