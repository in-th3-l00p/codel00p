# Slice: Shared dialog scaffolding (CLI simplification, Phase 2)

Date: 2026-06-16
Initiative: CLI simplification — "dialogs over commands".

## Goal

Extract the terminal lifecycle and blocking Elm loop that `config_ui` and
`memory_ui` each duplicated into one shared module, so every future dialog
(`skills`, `sessions`, `cloud`, `cron` in Phase 3) is just "build a model, hand a
draw + key handler to the helper" — a few lines instead of a copied loop.

## Design

- **`src/dialog.rs`** — `run_blocking<M>(model, draw, on_key)`: owns
  `setup_terminal`/`restore_terminal` (restoring on every exit path, including
  errors and `on_key` returning `Ok(false)`), key-release filtering, and now a
  panic hook the dialogs were missing (a panic mid-dialog no longer leaves the
  terminal in raw mode). `on_key(model, key) -> CliResult<bool>` returns
  `Ok(true)` to continue, `Ok(false)` to quit, `Err` to abort.
- **`config_ui/mod.rs`** — `run` becomes ~20 lines: build the model, stash the
  terminal `Flow` from `on_key` into an `outcome`, then `persist()` on Save.
- **`memory_ui/mod.rs`** — `run`'s `on_key` closure performs the repository
  effects (`Reload`/`OpenDetail`/`Mutate`) against the store it captures.
- The async chat TUI (`tui/event_loop.rs`) keeps its own loop — it multiplexes
  channel messages and captures the mouse, which the synchronous helper does
  not. (Sharing only the lifecycle there would change its mouse behavior for no
  real gain.)

## Tests

No behavior change: the pure `config_ui` (6) and `memory_ui` (7) update tests and
the `memory_cli` integration tests (20) all stay green; the dialog loop is
exercised by every dialog that uses it. fmt/clippy clean.

## Next (Phase 3)

`skills`, `sessions`, `cloud`, `cron` each get a small `*_ui/` on top of
`dialog::run_blocking`, plus the deferred in-dialog merge/restore for memory.
