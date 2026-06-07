# codel00p core

`core/` is the Rust workspace for the codel00p engine.

It contains the crates that should remain independent from any single user
interface:

- `codel00p-protocol`: shared runtime, memory, provider, tool, and session
  contracts.
- `codel00p-storage`: backend-neutral storage primitives and local SQLite
  backend.
- `codel00p-memory`: candidate lifecycle, review audit, and deterministic
  retrieval for durable project knowledge.
- `codel00p-providers`: inference provider profiles, policy, credentials, and
  transports.
- `codel00p-session`: durable session metadata and replay APIs.
- `codel00p-harness`: agent runtime, memory-aware inference context, tool loop,
  permissions, and lifecycle hooks.

## Commands

Run Rust checks from this directory:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo test -p codel00p-storage --features sqlite
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p codel00p-storage --features sqlite --all-targets -- -D warnings
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
