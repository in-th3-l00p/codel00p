# Deploying codel00p-cloud

The control-plane HTTP service. Runs on Fly.io (app `codel00p-cloud`, region
`iad`) and is **auto-deployed by `.github/workflows/deploy-cloud.yml`** on every
push to `main` that touches `core/**`, using the `FLY_API_TOKEN` repo secret.
The build runs on Fly's remote builder from `core/Dockerfile` (built with
`--features postgres`), so the runner needs no Docker.

## Environment

| Variable | Required | Purpose |
| --- | --- | --- |
| `CLERK_FRONTEND_API` *or* `CLERK_PUBLISHABLE_KEY` | yes | Locates the Clerk JWKS so the service can verify tokens. The publishable key is decoded to the frontend API host if the explicit host isn't set. |
| `DATABASE_URL` | **for durability** | A libpq connection string. **When unset the service runs in-memory and loses all data on every restart/deploy.** Setting it switches to Postgres. |
| `PORT` | no | Injected by Fly (8080). Defaults to 8787 locally. |

These are Fly **secrets** — set once, they persist across all future deploys, so
CI never needs to know about them.

## Make it durable (set `DATABASE_URL`)

The image already ships the Postgres backend, and `PostgresStorage::connect()`
creates its schema on boot (advisory-locked `CREATE TABLE IF NOT EXISTS`) — there
is **no separate migration step**. The driver supplies a TLS connector that
falls back to plaintext for internal servers and negotiates TLS for managed
hosts, so any Postgres provider works. You only need to point the service at one.

`flyctl` is required for the secret step: `brew install flyctl && fly auth login`.

### Option A — Neon (recommended: free, serverless, decoupled from Fly)

1. Create a project at <https://neon.tech> and copy the connection string.
2. ```sh
   fly secrets set DATABASE_URL="postgres://USER:PASS@HOST/db?sslmode=require" -a codel00p-cloud
   ```
   Setting the secret restarts the machine automatically.

### Option B — Fly-native Postgres (auto-wires the secret)

```sh
fly mpg create                              # or legacy: fly postgres create
fly mpg attach <db-name> -a codel00p-cloud  # provisions a DB/user and sets DATABASE_URL itself
```

## Verify

```sh
fly logs -a codel00p-cloud
# durable:   codel00p-cloud: using DATABASE_URL-backed storage
# ephemeral: codel00p-cloud: in-memory storage (set DATABASE_URL for durability)

curl https://codel00p-cloud.fly.dev/healthz   # -> ok
```

Then prove persistence: create a project (dashboard or CLI), redeploy
(`fly deploy` / `fly apps restart codel00p-cloud`), and confirm it's still there.

## Manual deploy

CI handles deploys, but you can deploy by hand from `core/`:

```sh
fly deploy --remote-only
```
