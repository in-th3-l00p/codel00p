# Slice: `?` help overlay + confirm-before-delete for the config/memory/sessions/cron dialogs

Date: 2026-06-16
Roadmap: Dialog polish — bring the synchronous full-screen dialogs to the same
state-of-the-art bar as the agent TUI.
Backlog/Initiative: Dialog UX polish (help discoverability + safe destructive actions).

## Goal

Every synchronous dialog (`config`, `memory`, `sessions`, `cron`) gains a uniform
`?` help overlay listing that dialog's real keybindings, and any immediate
destructive action confirms first. Concretely, cron's `x` delete now asks
`Press y to confirm, any other key to cancel` before removing a job; the other
dialogs either already prompt (memory's reject/archive gather a reason) or have no
immediate destructive action (config/sessions get the help overlay only).

## Why this slice

The dialogs already share a theme and a blocking Elm loop, but they had no
discoverable help and cron deleted jobs on a single keystroke. This slice is small,
self-contained (model + view per dialog plus one shared helper), and fully testable
with synthetic key events — no live credentials.

## Design

- `dialog.rs` (shared): add `pub(crate) fn centered(percent_x, percent_y, area)`
  so the overlay geometry is identical everywhere, and `render_help(frame,
  &[(key, description)])` which draws `Clear` + `panel("help")` + a `Paragraph`
  with the key in `accent()` and the description in `muted()`. The previously
  dead `error()` now has consumers, so its `#[allow(dead_code)]` is removed.
- Each model gains `show_help: bool`. `update` closes the overlay on ANY key while
  shown (and the underlying screen does not act), and `?` opens it. `?` is left as
  a literal character on text-entry screens (cron Create, memory Prompt, config
  provider fields) so reasons/URLs can still contain it — the help overlay is
  reachable from every navigational screen, which matches the shared spec.
- cron: `x` on the detail screen now arms `pending_delete: Option<String>` and
  shows an `error()`-styled confirm prompt instead of deleting; the next key `y`
  emits `Mutation::Delete`, anything else cancels. cron also routes failure status
  (failed run, store errors) through a new `set_error`/`status_is_error` path so the
  footer renders those in `error()`.
- Each view appends `? help` to its footer (cron's detail footer notes
  `x delete (confirms)`) and renders the overlay last when `show_help` is set.

## Tests (test-first, offline)

- cron: `?` toggles help and swallows the next key; `x` requires `y` to delete and
  cancels on any other key.
- config/sessions/memory: `?` toggles help and swallows the next key.
- memory: `?` is typed literally into the reason prompt (not treated as help).
- All pre-existing dialog tests stay green.

## Out of scope

Cloud and skills dialogs (owned by other agents this cycle) and any non-key
(mouse) interaction. No new destructive actions are introduced.
