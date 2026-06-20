# codel00p — Project Status

_Last updated: 2026-06-20 · Milestone: **Tooling-Complete (v0.7.0)**_

This is the at-a-glance project-management view: where the product stands, what
the current milestone covers, and what remains. Per-initiative detail lives under
[`docs/initiatives/`](docs/initiatives/); active ordering lives in the roadmap and
agentic-backlog docs.

## Milestone: Tooling-Complete (v0.7.0)

The agent's **tool-calling, programmatic execution, execution backends, and
end-to-end testing** are complete and released. This milestone marks the point
where the floor every mature coding agent ships — plus several differentiators —
is in place, tested, and governed.

| Metric | Value |
|---|---|
| Latest release | **v0.7.0** |
| Releases shipped | v0.1.0 → v0.7.0 |
| Workspace crates | 14 |
| Rust tests | ~1,300 |
| E2E scenario files | 21 |

## Initiative status

| # | Initiative | Status |
|---|---|---|
| — | End-to-end testing (`codel00p-e2e`) | ✅ Complete — 21 hermetic scenario files + coverage matrix |
| 10 | Tool-Calling Parity | ✅ Complete — schema/validation/truncation, `tool_choice`/`response_format`, MCP progressive disclosure, sub-agents, nav/grep/repo_map, background commands, plans, verbosity/pagination, streaming tool-call deltas |
| 8 | Programmatic Tool Calling | ✅ Complete — `run_pipeline` + sandboxed, governed `execute_code` |
| 7 | Execution Backends & Sandboxing | ✅ Core complete — `TerminalBackend` seam; local, Docker (ephemeral + warm), SSH backends; require-isolation-for-unattended policy. ⏸️ Cloud-sandbox backend deferred (needs a vendor + credentials) |
| 11 | Capability Synthesis | 🟡 Core shipped (freeze → verify → auto-extract); composition + org-propagation slices are the main remaining frontier |
| 1–6, 9 | Plugins, Skills, Self-Improvement, Sub-Agents, Scheduling, Gateway, TUI | Phase 1+ shipped (see initiative docs) |

## What's next (optional / post-milestone)

- **#11 capability flywheel** — capability composition (capabilities calling
  capabilities, bounded depth) + org propagation via reviewed team memory. The
  last remaining *differentiator* (not parity).
- **Cloud-sandbox backend (#7)** — a pluggable Daytona/Modal-style ephemeral
  sandbox; a clean drop-in against `TerminalBackend` once a vendor + credentials
  are chosen.
- **`execute_code` measurement (#8)** — quantify round-trip/token savings.
- **Live test layer** — env-gated real-provider/Docker/SSH smoke tests exist and
  are wired; run with credentials in a nightly/manual workflow.

## Working conventions

- Every change lands on its own branch in an isolated git worktree, with a PR,
  green CI, before squash-merge to `main`. Merged branches are pruned.
- Cargo runs from `core/`; do not run concurrent `cargo test` (a timing-sensitive
  MCP test). Live Docker/SSH backend tests are `#[ignore]`-gated and run serially.
- Releases are tag-driven (`v*` → `release.yml` builds CLI + desktop for 5
  platforms). The release matrix can hit transient crate-download flakes —
  re-run failed jobs.
