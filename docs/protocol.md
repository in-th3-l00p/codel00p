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
- `MemoryEntry`, `MemoryKind`, `MemoryStatus`, and `MemorySource`.

## Boundaries

The protocol crate should stay mostly data-only.

It should not own:

- provider credentials;
- provider transports;
- agent loop execution;
- tool implementation;
- memory storage;
- memory ranking;
- cloud persistence.

Those behaviors belong in the corresponding implementation crates. Protocol
types are the stable exchange format between them.
