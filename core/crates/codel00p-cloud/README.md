# codel00p-cloud

The team control-plane HTTP service. It is the source of truth for cloud product
data (projects today; provider policy, shared memory, and audit next), reusing
the engine crates and persisting through `codel00p-storage`.

Identity and membership come from **Clerk**: every request carries a Clerk
session JWT, verified at the boundary (RS256 against Clerk's JWKS) and projected
into an `AuthContext`. Clerk organizations are the team model; the service keys
its data by Clerk org/user IDs rather than owning a membership model.

## Endpoints

| Method + path                                    | Auth                 | Description                                   |
| ------------------------------------------------ | -------------------- | --------------------------------------------- |
| `GET /healthz`                                   | public               | Liveness.                                     |
| `GET /me`                                        | session              | The caller + active org (`Viewer`).           |
| `GET /projects`                                  | session + active org | Projects owned by the active org.             |
| `POST /projects`                                 | session + org admin  | Create a project (`NewProject`).              |
| `POST /projects/{id}/memory`                     | session + active org | Push a memory candidate (`NewMemoryCandidate`).|
| `GET /projects/{id}/memory?status=…`             | session + active org | List memory, optionally filtered by status.   |
| `POST /projects/{id}/memory/{mem}/approve`       | session + org admin  | Approve a candidate.                           |
| `POST /projects/{id}/memory/{mem}/reject`        | session + org admin  | Reject a candidate.                            |
| `GET /projects/{id}/memory/{mem}/audit`          | session + active org | Review audit trail (`MemoryAuditEntry[]`).     |

The memory review queue is the first shared-knowledge loop: a CLI/desktop
workspace pushes candidates, an org admin approves them, and clients pull the
approved set (`?status=approved`) back into future agent context. Every push and
review appends to a per-memory audit log.

Contracts live in `codel00p-protocol` and are mirrored to `@codel00p/protocol-ts`
(`Viewer`, `Project`, `NewProject`, `OrgRef`, `OrgRole`, `MemoryEntry`,
`NewMemoryCandidate`, `MemoryAuditEntry`, `MemoryReviewAction`).

## Storage

Storage is pluggable behind `codel00p-storage::StorageBackend`. The service
defaults to in-memory; set `DATABASE_URL` and build with `--features postgres`
to use Postgres:

```bash
DATABASE_URL=postgres://user:pass@host/db \
  CLERK_PUBLISHABLE_KEY=pk_test_… \
  cargo run -p codel00p-cloud --features postgres
```

Blocking storage runs on `spawn_blocking` threads so it never stalls the async
reactor.

## Running

```bash
# Derive the Clerk frontend API + JWKS from a publishable key…
CLERK_PUBLISHABLE_KEY=pk_test_… cargo run -p codel00p-cloud
# …or point at the frontend API host directly:
CLERK_FRONTEND_API=your-app.clerk.accounts.dev cargo run -p codel00p-cloud
# PORT defaults to 8787.
```

## Tests

- `auth` / `projects` unit tests — token-claim mapping (flat + compact `o`
  claim), org/role authorization, slugging, storage round-trips.
- `tests/http.rs` — end-to-end: a real axum server on an ephemeral port driven by
  `reqwest`, with RS256 tokens signed by a local test key standing in for Clerk.

Storage is in-memory today; the Postgres backend slots in behind
`codel00p-storage` without touching the handlers.
