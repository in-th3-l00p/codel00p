# Storage

`codel00p-storage` is the persistence boundary for local and cloud data.

The rule is simple: service crates should not call SQLite, Redis, cloud storage,
or object-store clients directly. Services own typed domain APIs, then store
data through backend-neutral storage traits.

## Primitives

The first API is intentionally small:

- `KeyValueStore`: scoped settings, cursors, leases, and lightweight runtime
  state.
- `DocumentStore`: structured records by scope, collection, and id.
- `AppendLogStore`: ordered streams for sessions, memory evolution, audit
  history, and sync.

This is a capability-based storage API. It is not an ORM and should not expose
SQL-specific concepts through service crates.

## Scope

Every primitive is scoped by `StorageScope`. A scope can represent global state,
a workspace, a user, or an organization/project pair. Backends can map this to
tables, Redis key prefixes, cloud tenant ids, or partition keys, but callers see
one stable model.

## Backend Strategy

The current backend is `InMemoryStorage`. It exists for tests, harness
development, and contract hardening.

Planned backends:

- SQLite for durable local project state.
- Redis for fast shared state, leases, queues, and ephemeral coordination.
- Cloud storage for organization-backed memory, audit, and team state.

Each backend should implement the same traits. Domain crates should continue to
depend on `codel00p-storage`, not on backend-specific packages.

## Service Pattern

Domain crates should build typed repositories on top of the primitives.

Example:

- `codel00p-session` stores `SessionMetadata` as documents.
- `codel00p-session` stores transcript records as append-log entries.
- future `codel00p-memory` should store approved memory entries as documents
  and memory-review/audit history as append logs.

This keeps storage replaceable while preserving strong domain APIs.
