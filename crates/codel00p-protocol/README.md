# codel00p-protocol

Shared Rust data contracts for codel00p modules.

This crate is intentionally small and behavior-light. It defines stable shapes
that the harness, providers, memory engine, CLI, desktop app, and cloud platform
can exchange without depending on each other's internal implementations.

Current contracts:

- protocol version marker;
- session, turn, and event IDs;
- session messages;
- tool calls and tool results;
- agent runtime events;
- project references;
- provider references;
- memory entries, kinds, statuses, and turn sources.

Provider credentials, transport behavior, storage, and memory ranking do not
belong in this crate.
