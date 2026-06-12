# Cron Run Executor

Second slice of [Initiative 5: Scheduling / Cron](../../initiatives/scheduling-cron.md),
Phase 1. Makes a scheduled job actually run.

## Goal

`codel00p cron run <id>` executes a saved job as an agent turn — safely, and
auditable like any other session.

## Scope

- [x] `agent::run_scheduled_job(config, defaults, job)`: builds an
      `AgentRunOptions` from the job (provider/model from the job, falling back
      to `agent.*` config; workspace from the job or cwd) and runs it via the
      existing `run_agent_turn`, so the run is persisted as a normal session.
- [x] Restricted by default: a scheduled run uses a **read-only** tool set, so a
      schedule can never silently edit files or run shell commands until that is
      deliberately opted into later.
- [x] `codel00p cron run <id>` wired in; refuses a disabled job. Cron dispatch
      moved after config resolution (it now needs the resolved config + agent
      defaults); management subcommands are unaffected.
- [x] Tests: a `cron_cli` integration test runs a job against a mock provider and
      asserts the agent's reply, and one asserting a disabled job is refused.
- [x] Help updated. `cargo test` (CLI single-threaded), `cargo fmt --check`,
      `cargo clippy`.

## Decisions

- **Restricted-by-default is the whole point of unattended execution.** The most
  dangerous surface is a schedule quietly running shell/edits, so scheduled runs
  are read-only until a future slice adds explicit, audited opt-in (and likely an
  isolating execution backend — initiative #7).
- **Reuse `run_agent_turn`.** The executor is a thin adapter from `CronJob` to
  `AgentRunOptions`; the run is a normal, persisted, auditable session — no
  parallel execution path.

## Out of scope (next slices)

- A per-run **hard timeout** (today bounded by `max_iterations`).
- A **background scheduler daemon** that wakes due jobs (this is what then powers
  the self-improvement curator and the sub-agent dispatcher).
- Opt-in elevated tool sets for trusted jobs, ideally inside an isolating
  execution backend.
- Output delivery (e.g. to a messaging gateway).
