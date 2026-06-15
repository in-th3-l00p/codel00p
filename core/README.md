# codel00p core

`core/` is the Rust workspace for the codel00p engine.

It contains the crates that should remain independent from any single user
interface. The 13 workspace members (see `core/Cargo.toml`):

- `codel00p-protocol`: shared session, event, tool, provider, memory, and cloud
  contracts (also the source for the TypeScript fixtures).
- `codel00p-storage`: backend-neutral storage primitives (document, key-value,
  append-log) with in-memory, SQLite, and Postgres backends.
- `codel00p-session`: durable session metadata and append-only event/message
  replay.
- `codel00p-memory`: candidate lifecycle, review audit, deterministic
  retrieval, quality scoring, and merge for durable project knowledge.
- `codel00p-providers`: inference provider profiles, policy, credentials,
  catalogs, and native streaming transports.
- `codel00p-harness`: the agentic turn loop — tools, permissions, memory
  injection, sub-agent delegation, skills, and lifecycle hooks.
- `codel00p-mcp`: Model Context Protocol client and server integration.
- `codel00p-cli`: the `codel00p` command-line interface and interactive TUI.
- `codel00p-cloud`: the team control-plane HTTP API (Axum service, Clerk auth).
- `codel00p-cron`: scheduling primitives — parse schedule specs and persist and
  run cron jobs.
- `codel00p-gateway`: the messaging gateway — one agent core reachable per
  conversation (e.g. Slack).
- `codel00p-skill`: skills — procedural memory codel00p can load and apply.
- `codel00p-plugin`: in-process plugin registry for extending the harness and
  provider registry.

## Commands

Run Rust checks from this directory:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo test -p codel00p-cli
cargo test -p codel00p-storage --features sqlite
cargo test -p codel00p-memory --features sqlite
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p codel00p-storage --features sqlite --all-targets -- -D warnings
cargo clippy -p codel00p-memory --features sqlite --all-targets -- -D warnings
```

From the repository root, the same checks are available through:

```bash
pnpm verify:core
```

## Boundary

The engine should not depend on `apps/` or `packages/`.

Apps and TypeScript packages may consume engine behavior through:

- CLI/native bridges;
- generated protocol bindings;
- cloud APIs;
- future FFI or process boundaries.

Do not duplicate agent runtime, provider routing, storage semantics, or protocol
contracts in app code.
