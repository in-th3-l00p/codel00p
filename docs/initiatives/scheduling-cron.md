# Initiative 5: Scheduling / Cron

## Goal

Let agents and users schedule codel00p work to run later or on a recurring
cadence — the prerequisite for an "always-on" agent, the curator
([#3](self-improvement-loop.md)), and the multi-agent dispatcher
([#4](subagents-delegation.md)).

## Why (Hermes reference)

Hermes ships built-in scheduling (`cron/jobs.py`, `cron/scheduler.py`):

- Agents schedule jobs via the `cronjob` tool; users via `hermes cron <verb>`.
- Accepts durations (`"30m"`), natural-language phrases (`"every monday 9am"`),
  and 5-field cron expressions.
- Jobs can run scripts pre-task, override the model/provider per job, and chain
  outputs.
- A hard 3-minute interrupt prevents runaway loops from monopolizing the
  scheduler.
- Results can be delivered across platforms (ties to [#6 Gateway](messaging-gateway.md)).

## Current codel00p state

- No scheduling. Every agent run is user-invoked and foreground.
- The pieces a scheduler needs exist: durable sessions (`codel00p-session`),
  config-layered model/provider selection, `--max-iterations` runaway bounding,
  and a storage layer for job persistence.

## Design

A new `codel00p-scheduler` crate (or module) plus a `cronjob` tool and
`codel00p cron` CLI.

### Job model
- Job record in `codel00p-storage`: id, project, schedule spec, prompt/goal,
  optional pre-task script, model/provider override, permission scope, output
  routing, enabled flag, last-run/next-run, run history.
- Schedule spec parser supporting duration, NL phrase, and 5-field cron.

### Scheduler
- A long-running scheduler process (`codel00p cron run` / a daemon) that wakes
  jobs, launches a harness session per job, and records the outcome as a normal
  session + event stream (fully auditable).
- Hard per-run interrupt (configurable, default mirrors Hermes's 3-minute cap)
  plus `--max-iterations` to bound runaway loops.
- Profile/project-aware: jobs are scoped per `project_id`, isolated state.

### Agent-facing tool
- `cronjob` tool (registered via [#1](plugins-and-hooks.md)) so an agent can
  schedule follow-up work ("re-run the test suite nightly", "remind me to
  review this PR Monday"), permission-gated like any other tool.

### CLI surface
- `codel00p cron list` / `add` / `remove` / `enable` / `disable` / `run` /
  `logs <id>`.

### Delivery
- Job output routes to: stdout/log (default), a session record, or — once
  [#6 Gateway](messaging-gateway.md) lands — a chat platform.

### Governance fit
- Scheduled runs are unattended, so they default to a **restricted permission
  scope** (e.g. read-only / no shell) unless the job explicitly opts in and the
  scope is allowed by org policy. Unattended shell access is the highest-risk
  surface and must be opt-in + audited.

## Scope

### Phase 1 — Job store & one-shot scheduling
- [x] `CronJob` record + duration schedule-spec parser + file-backed `JobStore`
      in a `codel00p-cron` crate, and `codel00p cron add/list/show/remove/
      enable/disable`. Slice:
      [2026-06-12-cron-job-store-and-cli](../superpowers/plans/2026-06-12-cron-job-store-and-cli.md).
- [x] `codel00p cron run <id>` foreground executor: runs the job as a fresh,
      read-only (restricted-by-default) agent turn, persisted as a normal
      auditable session; provider/model resolve from the job or `agent.*` config.
      Slice:
      [2026-06-12-cron-run-executor](../superpowers/plans/2026-06-12-cron-run-executor.md).
- [ ] Per-run hard timeout (currently bounded by `max_iterations`).
- [x] Background scheduler daemon driving due jobs: `codel00p cron daemon`
      (loop, `--interval`, `--once`) runs jobs whose next-run has passed, tracks
      `last_run` per job, marks-before-run for single-flight, and logs failures
      without stopping. Slice:
      [2026-06-12-cron-scheduler-daemon](../superpowers/plans/2026-06-12-cron-scheduler-daemon.md).
      This is the heartbeat the self-improvement curator and the sub-agent
      dispatcher build on.

### Phase 2 — Recurring + agent tool
- [ ] NL-phrase and 5-field cron parsing.
- [ ] `cronjob` tool for agent-scheduled work, permission-gated.
- [ ] Per-job model/provider override; pre-task script; restricted default scope.

### Phase 3 — Daemon & delivery
- [ ] Background scheduler daemon with durable next-run state.
- [ ] Output routing to gateway platforms ([#6](messaging-gateway.md)).
- [ ] Powers the curator ([#3](self-improvement-loop.md)) and dispatcher
      ([#4](subagents-delegation.md)).

## Risks & open questions

- **Unattended permissions**: scheduled shell/edit runs are dangerous; restrict
  by default, require explicit opt-in + org policy.
- **Daemon lifecycle**: where does the scheduler live (local daemon, desktop
  background process, cloud worker)? Likely all three over time; design the job
  store to be backend-neutral so a cloud worker can drive it later.
- **Overlap/concurrency**: prevent a slow job from stacking runs; single-flight
  per job.

## Dependencies

- [#1 Plugins & Hooks](plugins-and-hooks.md) (`cronjob` tool seam).
- Enables [#3 Self-Improvement](self-improvement-loop.md) and
  [#4 Sub-Agents](subagents-delegation.md) dispatcher.
- Pairs with [#6 Gateway](messaging-gateway.md) for delivery.

## Exit criteria

- A user or agent can schedule one-shot and recurring codel00p work with a
  restricted-by-default permission scope, runaway protection, per-job model
  overrides, and fully auditable run history.
