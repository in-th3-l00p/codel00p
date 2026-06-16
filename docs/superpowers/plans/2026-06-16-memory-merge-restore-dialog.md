# Slice: Memory merge + restore in-dialog actions (CLI simplification, Phase 1 follow-up)

Date: 2026-06-16
Roadmap: Milestone 6 (Interfaces) — "dialogs over commands".
Initiative: CLI simplification (completes a deferred Phase-1 item).

## Goal

Bring the two remaining `memory` mutations — **merge** and **restore** — into the
review dialog, so the interactive path no longer has to drop to the
`memory merge`/`memory restore` subcommands. These were left out of the original
memory dialog slice because each needs an extra selection step (a target record
for merge, an audit sequence for restore); this slice adds those steps.

## Why this slice

The memory dialog (`src/memory_ui/`) already covers approve/reject/archive/edit
from the Detail screen. merge and restore were the explicit "out of scope,
immediate follow-up" from `2026-06-16-memory-review-dialog.md`. Closing them
makes the dialog feature-complete versus the scriptable subcommands and keeps the
"bare invocation opens a dialog" convention honest.

## Design

Keeps the model **store-free**: every store read/write stays in the `mod.rs`
driver; the model only emits a `Flow`.

- **`model.rs`**
  - `AuditRow` gains `previous_content: Option<String>` (populated by the driver
    from `MemoryAuditEvent::previous_content`). Only rows carrying it are
    restorable.
  - New `RestoreRow` (`PickerItem`) projects restorable audit entries; built
    purely from `detail_audit`, newest sequence first.
  - New `Mutation::Merge { source, target }` and `Mutation::Restore { id,
    sequence }`.
  - New `Screen::SelectMerge` / `Screen::SelectRestore`, each driven by a
    `Picker` (`merge_targets`, `restore_picker`).
  - New `Flow::LoadMergeTargets(source)` — the model can't read the store, so `m`
    asks the driver to load candidate targets and hand them back via
    `show_merge_picker`. `u` is fully pure (audit is already loaded), so it opens
    the restore picker directly. Esc on either picker returns to Detail; empty
    candidate/history sets a status note and stays on Detail.
- **`mod.rs`** (driver)
  - `open_detail` now fills `previous_content`.
  - `load_merge_targets` lists Candidate+Approved records, drops the source, and
    calls `show_merge_picker`.
  - `apply` handles `Merge` via `MemoryRepository::merge(source, target,
    MemoryMerge::new(actor))` and `Restore` via a `restore` helper that mirrors
    the `memory restore` subcommand (find the audit sequence's `previous_content`,
    then `edit` the record back to it with a "restore audit sequence N" reason).
- **`view.rs`** — renders the two new picker screens and extends the Detail
  footer hint to `… · m merge · u restore · Esc back`, with per-screen footers.

## Tests

6 new pure `memory_ui::model` update tests (no store): `m` → `LoadMergeTargets`
then picker selection → `Mutate(Merge)`; empty targets stays on Detail; Esc
cancels the merge picker; `u` opens the restore picker (restorable entries only,
newest first) and selection → `Mutate(Restore{sequence:3})`; no-history stays on
Detail; Esc cancels the restore picker. All existing memory_ui + memory_cli
tests stay green; the scriptable `memory merge`/`memory restore` subcommands are
untouched.

## Out of scope

- Optional reason entry for in-dialog merge/restore (the CLI flags still allow
  it; the dialog uses a sensible default reason / none).
- Phase 3 dialog treatment for the remaining command groups.
