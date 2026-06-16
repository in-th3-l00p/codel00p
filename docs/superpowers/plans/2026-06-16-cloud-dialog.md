# Slice: Cloud dialog (CLI simplification, Phase 3c)

Date: 2026-06-16
Initiative: CLI simplification — "dialogs over commands".

## Goal

Make `codel00p cloud` (bare, on a TTY) open a dialog: show signed-in status,
browse the org's projects (and a project's agents / MCP servers / memory), and
offer push/pull as actions. The scriptable `status`/`push`/`pull`/`run`
subcommands stay byte-identical for non-TTY use. When not signed in, the dialog
shows a "run `codel00p auth login`" hint rather than erroring.

## Design

- **`src/cloud_ui/`** (on `crate::dialog`):
  - `model.rs` — pure, store-free. `Screen { Status, Detail, Unauthenticated }`,
    a `Picker<ProjectRow>` for the project list, three `Picker<EntityRow>`s
    (agents / MCP / memory) behind a `DetailTab` strip, and a transient `status`
    line for action results. `update(KeyEvent) -> Flow { Stay, OpenProject, Push,
    Pull, Quit }`. On the Status screen `p`/`l` are intercepted as push/pull
    before the picker's text filter; on Detail they fall through to filtering.
  - `view.rs` — pure rendering: a status panel (viewer summary + action line) over
    the project list; a tab strip + per-tab picker on Detail; a sign-in panel when
    unauthenticated. A single generic `draw_picker` renders every list.
  - `mod.rs` — the driver does **all** blocking IO: resolves the connection,
    fetches the viewer + projects up front, and on `OpenProject`/`Push`/`Pull`
    calls the blocking `CloudClient` directly. Fetch failures surface on the status
    line instead of aborting, so the dialog still opens.
  - `tests.rs` — 9 pure model tests (open project, quit, push/pull keys, ctrl-c,
    tab cycling, esc-from-detail, filter-doesn't-leak-actions, unauthenticated
    exit, status recording).
- **`cloud.rs`** — bare invocation opens the dialog when stdin+stdout are
  terminals (`std::io::IsTerminal`); non-TTY keeps the exact `missing cloud
  command` behavior. Extracted shared helpers reused by both the subcommands and
  the dialog: `collect_push_candidates` / `push_candidates` / `push_summary`,
  `pull_into_store` / `pull_summary`, and a flag-free `resolve_connection`
  returning a `CloudClient` + optional project (env first, then stored
  credentials). The `CloudClient` is a blocking reqwest client, so no async.
- **`main.rs`** — `mod cloud_ui;` registered.
- **`help.rs`** — cloud help now leads with the dialog and lists the subcommands
  under "For scripting"; the connection options/commands are unchanged.

The browser is read-only over entities; push/pull mutate via the existing
`cloud.rs` logic. Push/pull need a cloud project, taken from
`CODEL00P_CLOUD_PROJECT` (or stored); without one the action reports a hint.

## Tests

9 pure `cloud_ui::model` tests + the unchanged `cloud_cli` integration suite
(piped stdin/stdout, so the dialog never opens). `cargo test`,
`fmt --all -- --check`, `clippy --all-targets -D warnings` all green.

## Next (Phase 3)

`cron` (list + enable/disable/run/delete), plus in-dialog org switching and a
"resume agent run" action from the cloud browser.
