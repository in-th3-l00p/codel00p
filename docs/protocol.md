# Protocol

`codel00p-protocol` defines shared data contracts for codel00p modules.

The crate exists so the harness, provider layer, memory engine, CLI, desktop
app, and cloud platform can exchange stable serialized shapes without importing
each other's runtime internals.

## Current Scope

The protocol crate's modules and their key types:

- `version`: `ProtocolVersion`;
- `ids`: `SessionId`, `TurnId`, `EventId`;
- `session`: `SessionMessage`, `SessionRole`;
- `tools`: `ToolCall`, `ToolResult`;
- `events`: `AgentEvent`;
- `provider`: `ProviderRef`;
- `permissions`: `PermissionScope`, `PermissionMode`, permission decisions;
- `context`: `ContextWindowState` (context-window budgeting, consumed by the
  harness);
- `runtime` and `persistence`: shared runtime and session-persistence shapes;
- `memory`: `MemoryEntry`, `MemoryKind`, `MemoryStatus`, `MemorySensitivity`,
  `MemorySource`;
- `cloud`: the team control-plane contracts — `Project`, `Agent`, `McpServer`,
  `OrgRef`, `OrgRole`, `OrgMember`, `Viewer`, and the memory-review types.

Note: provider **policy preset** metadata lives in `codel00p-providers`
(`policy.rs`) and the `@codel00p/protocol-ts` TS package, not in this Rust
crate — see Boundaries below.

## Boundaries

The protocol crate should stay mostly data-only.

It should not own:

- provider credentials;
- provider transports;
- provider policy enforcement;
- agent loop execution;
- tool implementation;
- memory storage;
- memory ranking;
- cloud persistence.

Those behaviors belong in the corresponding implementation crates. Protocol
types are the stable exchange format between them.

`codel00p-harness` now uses protocol contracts for session IDs, turn IDs,
session messages, agent events, and model tool calls. This keeps the first
runtime aligned with the exchange format that future CLI, memory, desktop, and
cloud modules should consume.

The TypeScript protocol package also publishes the provider policy preset
catalog for app configuration surfaces. The metadata is descriptive only; the
Rust provider layer still owns policy construction and enforcement.
