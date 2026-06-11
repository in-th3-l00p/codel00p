# Protocol

`codel00p-protocol` defines shared data contracts for codel00p modules.

The crate exists so the harness, provider layer, memory engine, CLI, desktop
app, and cloud platform can exchange stable serialized shapes without importing
each other's runtime internals.

## Current Scope

The first protocol crate includes:

- `ProtocolVersion`;
- `SessionId`, `TurnId`, and `EventId`;
- `SessionMessage` and `SessionRole`;
- `ToolCall` and `ToolResult`;
- `AgentEvent`;
- `ProjectRef`;
- `ProviderRef`;
- built-in provider policy preset metadata for cloud, desktop, and SDK
  configuration surfaces;
- `MemoryEntry`, `MemoryKind`, `MemoryStatus`, and `MemorySource`.

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
