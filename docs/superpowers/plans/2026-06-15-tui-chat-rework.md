# Slice: TUI agent chat rework — scrolling, overflow, polished look

Date: 2026-06-15
Roadmap: Milestone 6 / TUI initiative — chat usability.

## Problem

The interactive TUI chat had three concrete defects: the transcript scroll math
counted logical lines but rendered wrapped rows, so the newest content was clipped
and there was no way to scroll back; the composer was a bare `String` with no wrap
and no cursor, so long input overflowed off the right edge and only end-of-line
editing worked; and messages were weakly differentiated by short text prefixes.

## What shipped

- **Wrap-aware scrolling** (`tui/view.rs`): `render` now takes `&mut App` and is the
  single source of truth for scroll. The transcript is pre-wrapped so each `Line` is
  one visual row; the renderer clamps a stored `ScrollState { offset_from_bottom,
  follow }` to the real height and pins to the bottom while following. PageUp/PageDown
  (`update.rs`) and the mouse wheel (`event_loop.rs map_event` → `Msg::Scroll`) scroll;
  reaching the bottom re-enables follow; submitting/resetting re-pins to latest.
- **Composer** (`tui/composer.rs`, new): an editable buffer with a char cursor —
  insert/backspace/delete at the cursor, ←/→, Home/End, Alt+Enter newline, `take()`
  on submit. `draw_input` char-wraps it, grows the box to a capped height, scrolls to
  keep the cursor visible, and draws a real terminal cursor via `set_cursor_position`.
- **Role-differentiated transcript** (`view.rs` `block_lines`): user/assistant blocks
  render a bold role header (`You` / `codel00p`) and body under a colored left accent
  bar (`▌`); tools are a compact glyph line; notices/errors get a colored gutter.

## Tests

- `composer.rs` unit tests (cursor editing, multibyte, newline, take).
- `update.rs` tests (PageUp leaves follow / PageDown resumes it, mouse scroll,
  Alt+Enter newline, submit re-pins, composer-backed typing/submit/clear).
- `view.rs` `TestBackend` tests: **newest line visible when overflowing** (the
  regression), scroll-up holds older content, long input wraps in the box, and the
  role labels/accent bar render.

## Verification

`cargo fmt --all -- --check`, `cargo test --workspace` (103 suites green),
`cargo clippy --workspace --all-targets -- -D warnings`. Rendering is covered by
`TestBackend` render tests (no real terminal needed); manual TTY check recommended
for feel.

## Out of scope (follow-ups)

- Mouse click-to-position cursor / row selection (mouse capture is on).
- Up/Down vertical cursor movement across wrapped composer rows.
- Input history (↑/↓ through prior prompts).
- Markdown rendering of assistant messages.
- TUI Esc-to-cancel of a running turn (tracked with the cancellation primitive).
