# Skill Curator

First slice of [Initiative 3: Self-Improvement Loop](../../initiatives/self-improvement-loop.md),
Phase 2 (curator). Closes the loop's far end: the agent's own skills are pruned
when they stop earning their place.

## Goal

Reversibly retire agent-created skills that are unused and past a grace period,
keeping the active set lean — without ever touching human work or deleting
anything.

## Scope

- [x] `codel00p-skill`: `archive_skill(root, name)` (move an active skill to
      `<root>/.archive`, no longer loaded) and `restore_skill`, plus the pure
      predicate `is_curatable(skill, usage, age_secs, min_age_secs)` —
      `created_by == agent` AND zero uses AND older than the grace period. New
      `UnknownSkill` error.
- [x] CLI `codel00p skills curate [--apply] [--min-age <dur>]`: dry-run lists the
      stale agent skills; `--apply` archives them. Uses the usage log and each
      skill's file mtime for age. `--min-age` takes a duration (`7d`) or bare
      seconds (`0` disables the grace period). Help updated.
- [x] Tests: crate unit tests (archive/restore round-trip, predicate only targets
      unused/old/agent skills) and a `skills_cli` integration test (a stale agent
      skill is listed then archived; a human skill is left alone).
- [x] `cargo test`, `cargo fmt --check`, `cargo clippy`.

## Decisions

- **Reversible, never deleting.** Curating moves a skill to `<root>/.archive`
  (skipped by the loader, so never injected) — recoverable via `restore_skill`
  or by moving the file back. This is codel00p's "never silently deletes" rule.
- **Only agent-created, only unused, only old.** `is_curatable` is conservative
  on all three axes, with a grace period (`min_age_secs`, default 7d) so a
  freshly-approved skill is not archived before it has had a chance to be used.
  Human-authored and bundled skills are categorically off-limits.
- **Dry-run by default.** `curate` lists candidates; `--apply` is required to
  act — review-gated, matching the rest of the self-improvement loop.

## The loop is now closed end to end

create/propose (explicit or auto-extracted) -> review/approve -> inject ->
**track usage** -> **curate the unused**. Each stage is governed and reversible.

## Out of scope (next)

- Running the curator automatically (a native command-job for the scheduler;
  today `skills curate --apply` runs under system cron).
- Snapshot/rollback beyond the archive move; reconciling with the existing
  memory staleness/similarity scoring and applying curation to memory.
- A `restore` CLI verb (the crate function exists; archived files can be moved
  back manually today).
