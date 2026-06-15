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

The current backends are:

- `InMemoryStorage` for tests, harness development, and contract hardening.
- `SqliteStorage` behind the `sqlite` feature for durable local project state.
- `PostgresStorage` behind the `postgres` feature for the hosted cloud service —
  deployed and verified durable in production (`codel00p-cloud` + a managed
  Postgres database on Fly.io). This is the "cloud storage for organization-backed
  memory, audit, and team state" backend; it requires no change to the domain
  crates, which still depend only on `codel00p-storage`.

Planned backends:

- Redis for fast shared state, leases, queues, and ephemeral coordination.

Each backend implements the same traits. Domain crates depend on
`codel00p-storage`, not on backend-specific packages.

## Service Pattern

Domain crates should build typed repositories on top of the primitives.

Example:

- `codel00p-session` stores `SessionMetadata` as documents.
- `codel00p-session` stores transcript records as append-log entries.
- `codel00p-memory` stores memory entries as documents and memory-review/audit
  history as append logs.

This keeps storage replaceable while preserving strong domain APIs.
