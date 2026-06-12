# Cron Scheduler Daemon

Third slice of [Initiative 5: Scheduling / Cron](../../initiatives/scheduling-cron.md),
completing Phase 1. Jobs now fire automatically.

## Goal

A background loop that runs each job when it is due, tracking per-job run state.

## Scope

- [x] `codel00p-cron`: a `last_run_epoch` field on `CronJob`, `CronJob::is_due(now)`
      and `due_jobs(jobs, now)` (enabled + valid schedule + never-run-or-past-next),
      and `JobStore::mark_ran(id, now)`. Scheduling math takes `now` as a
      parameter, so due-detection is pure and unit-tested.
- [x] CLI `codel00p cron daemon`:
  - `--once`: run one check now and exit (ideal under system cron / CI).
  - default: a foreground loop checking every `--interval` (default 60s).
  - Each tick marks a job as run **before** executing it (single-flight: a slow
    run is not double-fired by a later tick), runs it via the read-only executor
    from the previous slice, and logs a failure without stopping the daemon.
- [x] `cron show` now reports `last run`.
- [x] Tests: crate unit tests (`is_due` state transitions, `due_jobs` filtering)
      and a `cron_daemon --once` integration test (a due job runs against a mock
      provider, then a second tick finds nothing due — state is tracked).
- [x] Help updated. `cargo test`, `cargo fmt --check`, `cargo clippy`.

## Decisions

- **Pure due-detection, thin loop.** `is_due`/`due_jobs` are deterministic
  functions of `(job, now)`; the daemon is a thin wrapper that supplies wall-clock
  `now` and sleeps. The testable `--once` path is also genuinely useful for
  running under an external scheduler.
- **Mark-before-run.** Recording the run before executing prevents a slow job
  from being re-fired by the next tick, and means a perpetually-failing job runs
  at most once per interval (not every tick).
- **Never-run = due now.** A freshly added job runs on the next check, then every
  interval — simple and predictable.

## Phase 1 complete

Define -> run -> schedule is end to end: `cron add` / `run` / `daemon`. This
heartbeat is what the later **self-improvement curator** (retire stale agent
skills on a schedule) and the **sub-agent dispatcher** plug into.

## Out of scope (later)

- A per-run **hard timeout** (today bounded by `max_iterations`).
- A managed/OS service wrapper (launchd/systemd) around the foreground loop.
- NL phrases / 5-field cron expressions; per-job output delivery.
- The curator and dispatcher that consume this heartbeat.
