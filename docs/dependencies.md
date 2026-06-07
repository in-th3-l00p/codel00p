# Dependency Policy

codel00p should optimize for reproducible open-source development.

## JavaScript and TypeScript

Use `pnpm` from the repository root.

Rules:

- commit `pnpm-lock.yaml`;
- do not use `latest` in package manifests;
- use exact versions for app framework dependencies until release automation
  exists;
- use `workspace:*` for internal codel00p package dependencies;
- keep `@codel00p/protocol-ts` dependency-free;
- keep `@codel00p/ui` independent from `@codel00p/sdk`.

Run:

```bash
pnpm check:boundaries
pnpm typecheck
pnpm build
```

## Rust

Use the Cargo workspace under `core/`.

Rules:

- commit `core/Cargo.lock`;
- keep crates focused around product boundaries;
- prefer trait boundaries for replaceable infrastructure;
- verify feature-gated backends explicitly.

Run:

```bash
cd core
cargo fmt --all -- --check
cargo test --workspace
cargo test -p codel00p-storage --features sqlite
cargo clippy --workspace --all-targets -- -D warnings
```
