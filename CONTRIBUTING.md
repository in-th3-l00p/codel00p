# Contributing

codel00p is a working open-source project: a Rust engine (13 crates) behind a
shipping CLI/TUI, a hosted cloud control plane, and desktop/web surfaces.
Contributions should advance one of the active priorities below and keep the
knowledge-and-memory loop strong.

## Project priorities

The live working queue is [`docs/agentic-backlog.md`](docs/agentic-backlog.md);
the milestone and product views are [`docs/roadmap.md`](docs/roadmap.md) and
[`docs/product-roadmap.md`](docs/product-roadmap.md). Current standing
priorities:

1. provider route intelligence (catalogs, policy, fallback, cost);
2. memory quality (evidence, revision, duplicate/stale handling, merge);
3. agent parity (richer patch/diff, cancellation, web tools, worktree isolation);
4. MCP certification against common third-party servers;
5. team cloud (org-managed credentials, usage/budget, audit, sync);
6. desktop/cloud/terminal interfaces.

The Hermes-parity epics under [`docs/initiatives/`](docs/initiatives/README.md)
(plugins, skills, self-improvement, sub-agents, scheduling, gateway, TUI) have
shipped Phase 1 and continue in later phases. Cloud work is welcome when it
strengthens shared memory, provider governance, team visibility, or
organization workflows.

## Contribution style

Good contributions are:

- focused;
- easy to review;
- explicit about assumptions;
- aligned with the knowledge-first product direction;
- clear about which subproject they affect.

Documentation should stay concise and technical. Prefer short files with clear
boundaries over one large document.

Read [Repository Structure](docs/repository.md) before adding a new crate, app,
or shared package.

## Development

Rust engine work lives under `core/`:

```bash
cd core
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Application and TypeScript package work lives under `apps/` and `packages/`.
Use the root package manager workspace for those projects.

Run the repository check from the root before opening a pull request:

```bash
pnpm verify
```

`pnpm verify` runs `verify:apps` + `verify:core` (app/package boundary and
protocol-fixture checks, plus Rust `fmt`/`test`/`clippy` and the SQLite feature
tests). CI additionally enforces the Postgres tests
(`core:test:postgres`, `core:test:cloud:postgres`) and **100% storage line
coverage** (`storage-coverage.yml`), so run those locally if you touch
`codel00p-storage` or `codel00p-cloud`.

## Git history

Keep commits focused. A good commit should describe one coherent change. Branch
per change and open a pull request; CI (`core.yml`, `apps.yml`,
`storage-coverage.yml`) gates merges, and `deploy-cloud.yml` auto-deploys the
cloud when `core/` lands on `main`.

Do not add co-author trailers unless the maintainer explicitly asks for them.

## Development status

The engine, CLI/TUI, cloud service, and desktop/web surfaces are implemented and
running (see [`docs/pitch.md`](docs/pitch.md) for what is real today). Work now
is hardening and depth — provider breadth, memory quality, agent parity, MCP
certification, and the team-cloud governance surface — tracked in
[`docs/agentic-backlog.md`](docs/agentic-backlog.md).
