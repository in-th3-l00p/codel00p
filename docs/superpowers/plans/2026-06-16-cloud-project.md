# Cloud dialog: in-dialog project selection + `?` help overlay

Date: 2026-06-16
Branch: `feat/cloud-project`
Scope: `core/crates/codel00p-cli/src/cloud_ui/` only.

## Goal

Bring the `codel00p cloud` dialog to a polished finish:

1. Let the user pick the active cloud **project** inside the dialog, so the
   `p` push / `l` pull actions target that project's id directly — no
   `CODEL00P_CLOUD_PROJECT` round-trip required.
2. Add a shared-spec `?` help overlay matching the other dialogs.

## What changed

### Project selection (`model.rs`, `mod.rs`, `view.rs`)

- `CloudModel` gained an `active_project: Option<ProjectRow>` field, set by
  `show_detail` (opening a project) and by `set_active_project` (seeding from a
  preconfigured `CODEL00P_CLOUD_PROJECT` when it matches a listed project).
- The `Flow::Push` / `Flow::Pull` variants now carry the target project id
  (`Push(String)` / `Pull(String)`). On the status screen, `p`/`l` resolve the
  active project id and emit the targeted flow; with no active project they set a
  clear, non-error status ("Select a project first …") and stay.
- The driver (`mod.rs`) reuses `cloud.rs`'s `push_summary` / `pull_summary`
  against the chosen id. The old `CODEL00P_CLOUD_PROJECT`-only `active_project`
  helper and its `push_action` / `pull_action` wrappers were removed.
- The status panel now shows an "active project: …" line so the target is
  always visible.

### `?` help overlay (`model.rs`, `view.rs`)

- `show_help: bool` on the model; `?` toggles it. While shown, **any** key
  (including Esc) closes it and is otherwise swallowed (Ctrl-C still quits).
- Rendered as a centered overlay via `ratatui::widgets::Clear` +
  `crate::dialog::panel("help")` + a `Paragraph` of `key — description` lines
  (`accent()` for keys, `muted()` for descriptions), listing the current
  screen's keys.

### Tests (`tests.rs`)

Pure model tests added for: seeded-active-project targeting, open-project
targeting (survives returning to status), the select-a-project-first prompt, the
help toggle, help swallowing action keys, and Ctrl-C still quitting with help
open. Existing tests updated for the new `Flow` payloads; all kept green.

## Invariants held

- Model stays store-free; all blocking `CloudClient` calls happen in the driver.
- Shared theme (`crate::dialog::{accent, muted, selection, panel}`) used for all
  new UI.

## Gates

`cargo fmt --all -- --check`, `cargo test -p codel00p-cli`, and
`cargo clippy -p codel00p-cli --all-targets -- -D warnings` all green.
