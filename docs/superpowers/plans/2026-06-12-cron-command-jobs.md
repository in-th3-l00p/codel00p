# Cron Command Jobs

Slice spanning [Scheduling](../../initiatives/scheduling-cron.md) Phase 1 and
[Self-Improvement](../../initiatives/self-improvement-loop.md) Phase 2. Lets the
scheduler run maintenance commands, which makes the self-improvement loop
**autonomous** — the curator runs itself.

## Goal

Schedule a `codel00p` maintenance subcommand (not just an agent prompt) so upkeep
like `skills curate --apply` runs on the daemon without a human.

## Scope

- [x] `codel00p-cron`: a `command: Option<Vec<String>>` field on `CronJob`
      (`Some` => run a subcommand instead of the prompt), `JobStore::add_command`,
      and `CronJob::action()` for listings.
- [x] CLI `cron add-command <schedule> <subcommand...>`. The cron executor
      branches: a command job runs `codel00p <args>` in a subprocess
      (`current_exe`), an agent job runs the restricted agent turn as before.
      `cron list`/`show` render the command. Help updated.
- [x] Safety allow-list: command jobs may only run `skills`, `memory`, or
      `session` — never `agent`, `config`, or `providers`. Enforced at both
      `add-command` time and run time.
- [x] Tests: crate round-trip (command job persists, `action()` formats) and two
      `cron_cli` integration tests — a scheduled `skills curate --apply` command
      job, run via `cron daemon --once`, **archives a stale agent skill**; and a
      disallowed command (`agent run`) is rejected.
- [x] `cargo test`, `cargo fmt --check`, `cargo clippy`.

## Decisions

- **Subprocess, not internal re-dispatch.** A command job runs the binary again
  via `current_exe`, inheriting env (so `CODEL00P_HOME` etc. carry through). This
  reuses the whole CLI, isolates the command, and avoids re-entrancy in the
  dispatcher.
- **Narrow allow-list.** Unattended execution is the dangerous surface, so
  command jobs are restricted to safe maintenance subcommands. `agent` is
  excluded (that path already exists as an agent job, restricted read-only).

## Why this matters

The self-improvement loop was complete but manual at the far end. Now:
`cron add-command 1w skills curate --apply` retires stale agent skills on a
schedule — propose -> review -> inject -> track usage -> **curate, automatically**.

## Out of scope (later)

- A per-run hard timeout for command jobs (agent jobs share this gap).
- An OS service wrapper (launchd/systemd) for the daemon.
- Capturing/delivering command-job output (e.g. to a messaging gateway).
- The same command-job mechanism powering the sub-agent dispatcher.
