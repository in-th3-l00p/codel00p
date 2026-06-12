# Cron Job Store & CLI

First slice of [Initiative 5: Scheduling / Cron](../../initiatives/scheduling-cron.md),
Phase 1. The foundation: define and manage scheduled jobs.

## Goal

Model a scheduled job, parse schedule specs, persist jobs, and manage them from
the CLI — without yet running them (the executor and daemon are later slices).

## Scope

- [x] `codel00p-cron` crate: `parse_schedule` (duration intervals — `30m`, `2h`,
      `1d`, `1w`, optional `every` prefix — with unit aliases), a `Schedule` with
      a deterministic `next_after(now)` and `describe()`, a `CronJob` record
      (id, schedule, prompt, enabled, optional workspace/provider/model), and a
      file-backed `JobStore` (one `<id>.toml` per job, `cron-N` ids).
- [x] CLI `codel00p cron list | add <schedule> <prompt> | show <id> |
      remove <id> | enable <id> | disable <id>`, storing under `~/.codel00p/cron`.
- [x] Tests: crate unit tests (parse good/bad specs, `next_after`, store
      round-trip, add validation) and a `cron_cli` integration suite
      (empty list, add/list/show/disable/remove round-trip, bad schedule
      rejected). Help entry + blurb.
- [x] `cargo test`, `cargo fmt --check`, `cargo clippy`.

## Decisions

- **Duration intervals first.** `parse_schedule` handles `<n><unit>` (the common
  case) cleanly; 5-field cron expressions slot behind the same `Schedule` type
  later. `next_after(now)` takes `now` as a parameter, so scheduling math is
  deterministic and testable (no wall-clock in the crate).
- **Jobs as TOML files.** One file per job under `~/.codel00p/cron`, consistent
  with how skills and config are stored — transparent, git-friendly, no DB
  coupling. A cloud-backed store can come later behind the same `JobStore` shape.
- **Define now, run later.** This slice intentionally stops at job management.
  The executor (`cron run` as an agent turn) and a background daemon are the
  next slices, where unattended-permission safety is the central concern.

## Out of scope (next slices)

- `codel00p cron run <id>` executing the job as an auditable agent turn, with a
  restricted-by-default permission scope and a runaway interrupt.
- A background scheduler daemon driving due jobs (and powering the
  self-improvement curator and the multi-agent dispatcher).
- NL phrases (`every monday 9am`) and 5-field cron expressions.
- Per-job model/provider override wiring into the executor.
