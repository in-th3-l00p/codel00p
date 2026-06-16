# Slice: Sessions browser dialog (CLI simplification, Phase 3a)

Date: 2026-06-16
Initiative: CLI simplification — "dialogs over commands".

## Goal

Make `codel00p sessions` (bare, on a TTY) open a browser dialog: a filterable
list of persisted conversations, Enter to read a transcript. The scriptable
`session list`/`show` stay for non-TTY. The first Phase-3 group on the shared
`dialog::run_blocking` scaffolding.

## Design

- **`src/sessions_ui/`** (on `crate::dialog`): `model.rs` (pure `Picker<SessionRow>`
  + `Screen { List, Detail }` + `update(KeyEvent) -> Flow { Stay, OpenDetail, Quit }`,
  store-free), `view.rs` (list + scrollable transcript), `mod.rs` (driver: lists
  sessions via `open_session_store`, and on selection replays the transcript). The
  whole `run` is ~15 lines thanks to `dialog::run_blocking`.
- **`session.rs`** — bare invocation opens the dialog when stdin+stdout are
  terminals; non-TTY keeps the existing behavior. `agent_event_label` is now
  `pub(crate)` so the dialog reuses the same transcript formatting as `session
  show` (with `session_role_label`/`session_message_summary`).
- **`main.rs`** — `sessions` added as the primary verb alongside the `session`
  alias; help routes both.

This is a read-only browser (no mutations, no actor). Resuming a conversation
stays in the chat TUI's existing session switcher (F5 / `/switch`); a "resume from
the standalone browser" action is a follow-up (it would close the dialog and
launch chat, which needs the agent defaults threaded through dispatch).

## Tests

3 pure `sessions_ui::model` update tests (Enter → OpenDetail; Esc → Quit; detail
scroll clamps + Esc returns to list). `help_cli` updated for the new `sessions`
usage line. `cargo test`, `fmt --check`, `clippy -D warnings` green.

## Next (Phase 3)

`skills` (list + approve/reject candidates — closest to memory), `cloud` (reuse
the entity browser), `cron` (list + enable/disable/run/delete), plus the deferred
in-dialog merge/restore for memory and resume for sessions.
