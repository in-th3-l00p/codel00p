# Slice: Memory review dialog + inferred actor (CLI simplification, Phase 1)

Date: 2026-06-16
Roadmap: Milestone 6 (Interfaces) — "dialogs over commands".
Initiative: CLI simplification (the pattern-setter; see the approved plan).

## Goal

Collapse `memory`'s 13 flag-heavy subcommands into one dialog for interactive
use, and drop the most-hated required flag (`--actor`) from the human path —
without removing anything from the scriptable surface. This sets the convention
the rest of the CLI follows: **bare invocation on a TTY opens a dialog;
subcommands/flags stay for scripting; human inputs are gathered in the dialog or
inferred.**

## Why this slice

`memory` is the worst offender (13 subcommands; `--actor` required on every
mutation, plus `--reason`/`--content`/`--sequence`). It is the highest-leverage
place to prove the dialog convention, and it reuses the `config_ui/` template
plus the existing `Picker`/`Composer` widgets.

## Design

- **`src/actor.rs`** — `infer_actor()`: stored login email → `git config
  user.name` → `$USER`/`$USERNAME` → `"unknown"`. Read-only, infallible (runs on
  the default path of every mutation). `memory/review.rs` now does
  `actor.unwrap_or_else(crate::actor::infer_actor)` in all four mutations, so
  `--actor` is optional but still honored when given.
- **`src/memory_ui/`** (mirrors `config_ui/`): `model.rs` (pure state +
  `update(KeyEvent) -> Flow`, store-free), `view.rs` (pure render), `mod.rs`
  (blocking driver that opens the store once and performs `Flow::{Reload,
  OpenDetail,Mutate}` effects), `tests.rs` (pure update tests). Screens:
  **List** (browse, type-to-filter, `Tab` cycles status, `Enter` → detail),
  **Detail** (record + audit; `a` approve · `r` reject · `x` archive · `e` edit),
  **Prompt** (a `Composer` gathers reason/content inline). Reusing `Picker` for
  the list cleanly separates filtering (List) from single-key actions (Detail).
- **`memory.rs`** — bare `codel00p memory` opens the dialog when stdin+stdout are
  terminals; non-TTY keeps the existing "missing memory command" behavior, so
  pipes/CI never open a dialog. The subcommand table is untouched.
- **`tui/composer.rs`** — `set_text` un-gated from `#[cfg(test)]` (now a real
  feature: pre-filling an edit with current content).

## Tests

`actor.rs` precedence unit test; 7 pure `memory_ui::model` update tests (filter
cycle → Reload; approve immediate; reject/edit prompt → Mutate; empty reason
stays; Esc cancels; edit pre-fills); a `memory_cli` integration test asserting
`memory approve <id>` succeeds with **no** `--actor` (inferred). All existing
`--json`/`--actor` scripting paths unchanged (19 prior tests still green).

## Out of scope (immediate follow-ups)

- **merge** and **restore** as in-dialog actions (they need a target/sequence
  picker) — available via the CLI subcommands meanwhile.
- Phase 2: extract shared `dialog.rs` scaffolding (dedupe the terminal loop
  across `config_ui`/`memory_ui`).
- Phase 3: the same dialog treatment for `skills`, `sessions`, `cloud`, `cron`.
