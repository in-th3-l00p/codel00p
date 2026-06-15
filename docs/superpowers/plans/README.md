# Slice plans

This directory is the append-only record of the **dated slice plans** the
agentic development loop writes before each change. Each file captures one
shippable slice: its goal, why it was chosen, the design, the test plan, and what
was left out. They are a history, not living docs — once written they are not
rewritten (later slices get new files).

See [`../../agentic-backlog.md`](../../agentic-backlog.md) for the live queue and
[`../../initiatives/`](../../initiatives/README.md) for the multi-phase epics that
these slices implement.

## Convention

- **Filename:** `YYYY-MM-DD-slug.md`, lowercase and hyphenated (e.g.
  `2026-06-15-memory-merge.md`). All current files follow this.
- **Template:** new plans should follow [`_TEMPLATE.md`](_TEMPLATE.md)
  (Goal / Why this slice / Design / Tests / Out of scope). Older plans predate
  the template and use looser shapes — that is expected for the archive.
- **Discovery:** the directory is flat; browse with `ls`, or
  `git log --diff-filter=A -- docs/superpowers/plans/` for the order things
  landed.

## Themes

The ~110 plans cluster into a few areas (counts approximate; use
`ls | grep <keyword>`):

- **Memory** (`*memory*`, ~34) — extraction, edit/restore, audit, similarity,
  staleness, quality scoring, sensitivity, and merge.
- **Providers & routing** (`*provider*`, `*catalog*`, `*policy*`, `*route*`,
  ~23) — model catalogs, capability/cost metadata, fallback routing, and policy.
- **Credentials & enterprise policy** (`*credential*`, `*managed-identity*`,
  `*enterprise*`) — managed-identity resolvers and org/gateway policy templates.
- **CLI & config** (`*cli*`, `*config*`) — command surfaces and layered config.
- **MCP** (`*mcp*`) — client/server transports and memory tooling over MCP.
- **The 2026-06-12 initiative wave** — `*cron*`, `*plugin*`, `*skill*`,
  `*subagent*`, `*gateway*` (Phase 1 of the [initiatives](../../initiatives/README.md)).
- **TUI & org** (`*tui*`, `*org*`) — the ratatui terminal UI and org entities.

## Design specs

Longer-form design docs (as opposed to slice plans) live in
[`../specs/`](../specs/). Today that is a single harness design spec; cross-link
new specs from the slice plan that consumes them.
