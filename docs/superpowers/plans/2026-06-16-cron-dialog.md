# `codel00p cron` dialog — Phase 3d (dialogs over commands)

## Goal
Bare `codel00p cron` on a TTY opens a full-screen dialog for managing scheduled
jobs, mirroring the established `config_ui` / `memory_ui` / `sessions_ui` dialogs.
The `daemon` subcommand and every scriptable subcommand stay byte-identical for
non-TTY use.

## Shape (Elm-style, store-free model)
New `core/crates/codel00p-cli/src/cron_ui/`:

- `model.rs` — pure `CronModel::update(KeyEvent) -> Flow`. Screens: `List`,
  `Detail`, `Create`. `Flow` ∈ {`Stay`, `OpenDetail(id)`, `Mutate`, `RunNow(id)`,
  `Quit`}. `Mutation` ∈ {`SetEnabled`, `Delete`, `Add`}. Reuses
  `tui::picker::Picker` for the list and `tui::composer::Composer` for create
  input. The model never touches the store or agent.
- `view.rs` — pure rendering: list (id + action + schedule/state), detail (full
  job config: schedule, enabled, workspace/provider/model, last run, prompt or
  command), and the two-step create form (schedule, then prompt).
- `mod.rs` — ~15-line driver via `crate::dialog::run_blocking`; the `on_key`
  closure performs store effects (`JobStore::{list,get,set_enabled,remove,add}`)
  and "run now" (`crate::cron::execute_job`, made `pub(crate)`), reusing the exact
  operations behind the subcommands.
- `tests.rs` — pure key-driven tests of the transitions.

## Interactions
- List: ↑/↓ move, type to filter, Enter opens detail, `n` new job, Esc quits.
- Detail: `e` enable, `d` disable, `R` run now, `x` delete, Esc back.
- Create: prompts for schedule (e.g. `30m`) then the agent prompt, then
  `Mutation::Add` (reuses `JobStore::add` + `parse_schedule`).

## Dispatch + gate
In `cron::run`, when there are no args and both stdin and stdout are terminals
(`std::io::IsTerminal`), open `crate::cron_ui::run(config, defaults)`; otherwise
keep the exact current default (`cron_list()`). Non-TTY never opens the dialog.
`mod cron_ui;` registered in `main.rs`. `execute_job` is now `pub(crate)` so the
dialog runs jobs identically to `cron run`.

## Help
`CRON_HELP` now leads with the dialog, then "For scripting (and pipes/CI)" lists
the subcommands. No `tests/help_cli.rs` usage assertion needed updating.

## Gates
`cargo fmt --all -- --check`, `cargo test -p codel00p-cli`,
`cargo clippy -p codel00p-cli --all-targets -- -D warnings` all green.
