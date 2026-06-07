# codel00p-session

Durable session storage contracts for codel00p.

This crate owns session-specific metadata, transcript records, lineage, and
replay APIs. It does not own concrete storage engines. `StorageBackedSessionStore`
persists metadata and ordered records through `codel00p-storage`, so SQLite,
Redis, cloud storage, or test memory stores can sit behind the same session API.

The crate persists durable transcript and boundary records. High-frequency
progress ticks are intentionally runtime events, not replay records, unless a
caller explicitly appends them.
