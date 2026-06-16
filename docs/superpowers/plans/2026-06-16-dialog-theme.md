# Slice: Shared dialog color theme (CLI polish foundation)

Date: 2026-06-16
Initiative: CLI polish — make the six full-screen dialogs look modern and
consistent with the chat TUI.

## Goal

The six dialogs (`config`, `memory`, `sessions`, `skills`, `cloud`, `cron`) each
defined identical *monochrome* styles (`BOLD` accent, `REVERSED` selection, `DIM`
muted, plain borders). The chat TUI already ships a real 256-color palette at
`tui/theme.rs`. Centralize the dialog styles onto that same `Theme` so the
dialogs pick up the soft-blue accent, grey selection chip, and themed border —
matching the chat. Purely visual: no behavior, layout, or keybinding changes.

## Design

- **`src/dialog.rs`** gains `pub(crate)` style helpers sourced from
  `crate::tui::theme::Theme::default()` (cheap to construct):
  - `accent()` → `Theme::accent()` (soft-blue, bold)
  - `muted()` → `Theme::muted()` (mid grey)
  - `selection()` → `Theme::selection()` (grey chip, accent fg, bold)
  - `error()` → fg = `theme.error` (soft red)
  - `panel(title)` → bordered `Block<'static>` with `border_style` =
    `theme.overlay_border` and an accent-styled `" {title} "` title span.
- **All six `*_ui/view.rs`** drop their local `ACCENT` const and local
  `selected`/`muted`/`block` fns, `use crate::dialog::{accent, muted, panel,
  selection}` (only what each needs), and swap usages: `ACCENT` → `accent()`,
  `selected()` → `selection()`, local `block(..)` → `panel(..)`. The `cloud`
  tab strip keeps its `accent().add_modifier(UNDERLINED)`. The `.block(..)`
  Paragraph *method* calls were left untouched — only the local `block(..)`
  helper calls became `panel(..)`. Now-unused imports (`Modifier`, `Borders`,
  `Block`) were removed so `clippy -D warnings` stays clean.
- `error()` has no consumer yet (no dialog surfaces an error state), so it
  carries an `#[allow(dead_code)]` with a note — it is the shared error style
  for the upcoming polish slices, not re-derived per dialog.

## Gates

`cargo fmt --all -- --check`, `cargo test -p codel00p-cli` (306 passed, the
existing dialog tests are behavior tests and unaffected), and
`cargo clippy -p codel00p-cli --all-targets -- -D warnings` all green.

## Next

Per-dialog polish on top of the shared palette (status/error lines using
`error()`, spacing, scroll affordances).
