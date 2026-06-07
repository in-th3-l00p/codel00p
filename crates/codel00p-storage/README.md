# codel00p-storage

Backend-neutral storage contracts for codel00p.

Services should not depend on SQLite, Redis, object storage, or cloud APIs
directly. They should expose typed domain repositories and persist through this
crate's small set of primitives:

- `KeyValueStore` for scoped settings, cursors, leases, and lightweight state.
- `DocumentStore` for structured records by scope, collection, and id.
- `AppendLogStore` for ordered session, memory, audit, and sync streams.

The first backend is `InMemoryStorage`, used to lock down contracts and make
domain crates testable without external services. SQLite, Redis, and cloud
storage should implement the same traits as separate backend modules without
changing service APIs.

The design goal is capability-based storage, not a SQL-shaped abstraction.
Different backends can be excellent internally while codel00p services keep a
stable API for local and cloud persistence.
