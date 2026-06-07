# Repository Structure

codel00p is a product monorepo with one Rust engine workspace and separate
application/package workspaces.

```text
core/      Rust engine workspace
apps/      deployable user-facing applications
packages/  shared TypeScript packages
docs/      product, architecture, and research documentation
tools/     repository automation and fixtures
```

## `core/`

`core/` owns engine behavior:

- protocol contracts;
- storage contracts and backends;
- inference provider routing;
- session replay;
- harness runtime behavior;
- future memory and sync engines.

Rust crates live under `core/crates/`. Cargo commands should run from `core/` or
through the root `pnpm verify:core` script.

## `apps/`

`apps/` contains deployable surfaces:

- `landing`: public Next.js website;
- `cloud`: organization and team web platform;
- `desktop`: Electron control center.

Apps can depend on `packages/*`. Apps should not become homes for shared product
logic.

## `packages/`

`packages/` contains reusable TypeScript:

- `ui`: shared React components;
- `protocol-ts`: TypeScript protocol contracts;
- `sdk`: client SDK;
- `config`: shared configuration.

Package rules:

- packages cannot depend on apps;
- internal package dependencies must use `workspace:*`;
- `protocol-ts` should remain dependency-free;
- `ui` must not depend on `sdk`;
- apps may depend on packages.

These rules are enforced by `pnpm check:boundaries`.

## Protocol Drift

`packages/protocol-ts` must stay aligned with `core/crates/codel00p-protocol`.

The current drift check validates Rust protocol fixtures against the TypeScript
contract manifest:

```bash
pnpm check:protocol
```

Long term, TypeScript contracts should be generated from Rust-owned schemas or
verified with richer fixture conformance tests.

## Verification

Run the full repository verification from the root:

```bash
pnpm verify
```

This runs application checks, package boundary checks, protocol fixture checks,
Rust tests, SQLite feature tests, and clippy.
