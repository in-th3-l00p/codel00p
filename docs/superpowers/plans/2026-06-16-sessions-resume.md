# Slice: Sessions browser resume action (CLI simplification, Phase 3a)

Date: 2026-06-16
Initiative: CLI simplification — "dialogs over commands".

## Goal

Complete the deferred "resume from the standalone browser" item noted in
`2026-06-16-sessions-dialog.md`: pressing `r` in the `codel00p sessions` browser
(on a selected row in the List screen, or while reading a transcript in Detail)
continues that conversation in the chat TUI — exactly as
`codel00p agent chat --session-id <id>` would, full-screen.

## Design

- **`src/sessions_ui/model.rs`** — new `Flow::Resume(String)` (the session id).
  `update_list` intercepts `r` before the picker and returns `Resume(<selected id>)`;
  `update_detail` adds an `r` arm returning `Resume(<open id>)`. The model stays
  store-free, so the transition is unit-testable from synthetic key events.
- **`src/sessions_ui/view.rs`** — footers mention `r resume` on both screens.
- **`src/sessions_ui/mod.rs`** — `run` now takes `&AgentSettings`. The blocking
  loop captures the resume id and returns `Ok(false)` so the dialog tears the
  terminal down first; chat is launched *after* via `crate::agent::resume_chat`.
  Reuses the one chat-launch path — no chat loop is reimplemented.
- **`src/agent/command.rs`** — `pub(crate) fn resume_chat(config, defaults, id)`
  delegates to the existing `agent_chat` with `--session-id <id>`, re-exported
  from `agent.rs`. Honors the same TTY/`--json-events` fallback as `agent chat`.
- **`src/session.rs` / `src/main.rs`** — `agent_defaults` is threaded into
  `session::run(config, agent_defaults, rest)` so the bare-`sessions` dialog can
  build chat options. Non-TTY scriptable `list`/`show` are unchanged.

## Tests

2 new pure `sessions_ui::model` tests: `r` on a selected List row, and `r` in
Detail, each yield `Flow::Resume(<id>)`. Existing model/view/CLI tests stay green.
`cargo fmt --check`, `cargo test -p codel00p-cli`, `cargo clippy -p codel00p-cli
--all-targets -- -D warnings` all clean.

## Notes / trade-offs

`r` is consumed as an action key on the List screen, so it cannot be typed into
the incremental filter. This matches the explicit `r resume` affordance in the
footer; navigation/open (arrows, Enter, Esc) are unchanged.

## Next (Phase 3)

The remaining deferred Phase-3 items: in-dialog merge/restore for memory, and the
`cron` list + enable/disable/run/delete browser.
