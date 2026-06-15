# Tools

Repository automation, scripts, fixtures, and release helpers.

`scripts/`:

- `verify.sh` — the `pnpm verify` entrypoint; runs `verify:apps` + `verify:core`.
- `check-package-boundaries.mjs` — enforces the app/package dependency boundary.
- `check-protocol-fixtures.mjs` — checks the TypeScript protocol fixtures stay
  aligned with `core/crates/codel00p-protocol`.

Keep product runtime code in `core/`, `apps/`, or `packages/`; this directory is
only for development operations.
