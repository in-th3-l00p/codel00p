# codel00p-session

Durable session storage contracts for codel00p.

The first backend is an in-memory append log used to lock down the API and test
replay semantics. A SQLite backend should implement the same `SessionStore`
trait later, with searchable records, parent session lineage, and compact
boundary metadata.

The crate persists durable transcript and boundary records. High-frequency
progress ticks are intentionally runtime events, not replay records, unless a
caller explicitly appends them.
