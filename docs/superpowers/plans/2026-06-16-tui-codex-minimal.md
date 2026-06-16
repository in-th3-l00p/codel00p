# Slice: minimal Codex-style chat + command palette

Date: 2026-06-16
Roadmap: Milestone 6 / TUI initiative — chat usability & look.

## Context

The chat had bordered panels (conversation + input) and colored role headers, and
every action was behind a separate F-key surface (F2/F3/F5). The ask: a more
minimal, Codex-like look (no borders, background-based separation) and a single
VSCode-Ctrl+P-style command menu instead of the F-key UIs.

## What shipped

- **No borders.** The transcript renders transparent (no box/title, 1-col margin).
  The composer is a **filled background block** (`theme.input_bg`) with no border and
  a `›` prompt; the status line sits below it.
- **Background-based role differentiation.** Message text is white; **user messages
  get a subtle full-width background tint** (`theme.user_bg`) with a dim `›` marker,
  while assistant output stays transparent (and markdown-rendered). The colored role
  bars/headers are gone.
- **Command palette** (`Ctrl+P`): a filterable modal (`overlay::CommandPalette`,
  reusing `Picker`) listing every action — switch model, switch session, new
  conversation, browse organization, browse users, switch organization, history,
  tools, help, quit. Selecting one dispatches to the existing handler. It is the
  primary launcher; the status bar advertises `Ctrl+P menu`, and F-keys remain as
  hidden shortcuts.
- **Multiline composer** stays (Enter sends, Alt/Shift+Enter newline, grows/scrolls).

## Files

- `tui/theme.rs` — add `input_bg`/`user_bg`; drop the now-unused role colors.
- `tui/overlay.rs` — `CommandAction`, `CommandItem`, `CommandPalette`,
  `Overlay::Command`, `command_items()`.
- `tui/update.rs` — `Ctrl+P` opens the palette; `run_command` dispatches selections.
- `tui/view.rs` — transparent transcript, background user blocks (`pad_to_width`),
  borderless filled composer, `draw_command`, status/help text.

## Tests / verification

- `update`: Ctrl+P opens the palette; selecting "Switch model" opens the model
  picker; filtering to "quit" + Enter returns `Effect::Quit`.
- `view` (`TestBackend`): user message carries the `user_bg` cell background; the
  palette renders its actions; user + assistant both render.
- `cargo fmt --check`, `cargo test --workspace` (103 suites), `clippy -D warnings`.

## Out of scope

- Configurable / light theme; click-to-run palette rows; fuzzy (subsequence)
  matching in the palette (it uses the existing substring filter).
