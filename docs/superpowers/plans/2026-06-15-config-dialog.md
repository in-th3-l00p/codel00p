# Slice: Interactive `codel00p config` dialog + first-run setup

Date: 2026-06-15
Roadmap: Milestone 6 (Interfaces) / Milestone 1 (first-run UX).
Backlog/Initiative: onboarding — "that's the entire configuration you need".

## Goal

Make configuration a single, full-screen dialog. `codel00p config` opens it
(Providers · Tools · Permissions); `codel00p config providers` opens it on the
Providers section. The first time any command runs unconfigured, codel00p walks
the user through the same dialog before continuing. `config` reopens it to change
anything later.

## Why this slice

A dialog is the configuration surface users asked for: discoverable, navigable,
and the same whether you're onboarding or tweaking. It builds directly on the
provider catalog/credential/settings plumbing (and reuses the line wizard as the
non-TTY fallback), so it's additive rather than a rewrite.

## Design

`core/crates/codel00p-cli/src/config_ui/` (Elm-style, mirroring the chat TUI):

- `model.rs` — pure state + `update(KeyEvent) -> Flow`. Sections: a menu, a
  provider screen (a provider list plus editable API-key/model/base-URL fields
  with `Tab` focus), a tool-set checklist, and a permission-mode radio. The draft
  is seeded from the currently effective settings via `settings::effective_value`,
  so the dialog edits the existing config. `persist()` writes
  `agent.provider/model/base_url/permission_mode/tool_sets` (the last coerced to a
  TOML array) and any entered key into `~/.codel00p/.env`.
- `view.rs` — pure ratatui rendering.
- `mod.rs` — terminal lifecycle + a blocking event loop, restoring the terminal
  on every exit path.

Dispatch (`main.rs`): bare `config` and bare `config providers` open the dialog
when stdin+stdout are a TTY; otherwise the existing scriptable subcommands run. A
first-run hook runs the dialog before any non-config command when no provider is
configured yet and the shell is interactive. Non-TTY/CI is unaffected.

## Tests (test-first)

Pure `update` tests driving synthetic key events over an isolated temp
`CODEL00P_HOME`: menu navigation opens each section; tool toggle adds/removes;
permission select updates the mode; provider select + key entry sets the pending
key; `Tab` cycles fields and edits the model; `s` / menu rows drive Save/Quit.

## Out of scope

Live `list_models` inside the dialog (the line wizard has it; the dialog uses
free-text model entry prefilled from the provider default), the agent run-mode /
execution-backend section (deferred until backends exist), mouse support, and
themes.
