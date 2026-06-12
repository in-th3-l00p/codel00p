# Skill Usage Tracking

First slice of [Initiative 3: Self-Improvement Loop](../../initiatives/self-improvement-loop.md),
Phase 1 (usage signal). The prerequisite for the curator: know which skills are
actually used.

## Goal

Record how often each skill is applied and when, and surface it — so a later
curator can retire stale agent-created skills, and so users can see what's
earning its place in context.

## Scope

- [x] `codel00p-skill` `usage` module: `SkillUsage` (count + `last_used_epoch`),
      a `UsageLog` per skills root persisted as `.usage.json`, `load_usage`,
      `record_usage(root, name, now)`, and `record_skill_usage(skill, now)` which
      derives the skill's root from its path so usage is logged in its own root.
- [x] CLI: `CliSkillProvider` records each selected skill's usage **once per
      turn** (the provider is built per turn; an in-memory set dedups across the
      agentic loop's iterations). Recording is best-effort — it never fails a
      turn. `skills list` shows `used Nx` / `unused`.
- [x] Tests: crate unit tests (record/load, root derivation) and an end-to-end
      `agent_cli` test — a skill is `unused`, an agent run injects it, and
      `skills list` then shows `used 1x`.
- [x] `cargo test`, `cargo fmt --check`, `cargo clippy`.

## Decisions

- **`.usage.json` per root, matching the Hermes shape.** Simple, transparent,
  and co-located with the skills it describes. Read-modify-write, last-writer-
  wins — fine for a local single-user store; a concurrent-safe store can come
  later behind the same functions.
- **Once per turn, not per iteration.** `select` runs each loop iteration; the
  provider dedups by skill name for its (per-turn) lifetime, so a skill used in a
  turn counts once regardless of how many iterations ran.
- **Best-effort.** Usage is observability, never correctness — a failed write is
  ignored so it can never break an agent turn.

## Out of scope (next slice — the curator)

- The **curator**: a scheduled job (now that `cron daemon` exists) that flags
  `created_by: agent` skills unused past a threshold as **archive candidates**
  (review-gated, never silently deleted), leaving bundled/human-approved skills
  alone.
- `last_used` recency surfaced in `skills list` / `show` (only counts shown now).
- The same usage signal for project memory.
