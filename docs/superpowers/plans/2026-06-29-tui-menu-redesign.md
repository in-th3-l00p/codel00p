# TUI Ctrl+P menu redesign

Replace the flat 13-item Ctrl+P command palette with a clean, hierarchical
four-section menu — **Agent · Conversations · Organization · Settings** — where
each section is a focused area with a "create new" first row, per-item
edit/delete/inspect actions (`e` / `d` / Enter), and detail views.

## Why

The flat palette listed every action at one level ("Switch model", "Switch
session", "Browse organization", "Show history", …). It was too much to scan and
mixed unrelated concerns. Grouping into four durable sections matches how users
actually think about the tool and leaves room for richer per-section UX.

## Architecture

- Ctrl+P opens `Overlay::Menu(MainMenu)` — a `Picker<MenuSection>` over the four
  sections. Selecting a section opens its dedicated overlay.
- Reuses the existing `Picker<T>` primitive and the `framed()` overlay chrome, so
  every section list looks and behaves identically.
- The per-section overlays (agent switcher, sessions, entity/org browser,
  settings) already exist; the redesign restructures the entry point first, then
  enriches each overlay in place.

## Phases

### Phase 1 — top-level menu  ✅ (commit b469d13)
- `MenuSection` enum + `MainMenu`; `Overlay::Command` → `Overlay::Menu`.
- `open_command_palette`/`run_command` → `open_menu`/`open_section`.
- `draw_command` → `draw_menu`. Tests + clippy `-D warnings` green (118 TUI tests).
- Routing: Agent → agent switcher, Conversations → sessions, Organization → org
  browser, Settings → settings.

### Phase 2 — Agent section  🟡 (create + delete shipped; edit + detail next)
- ✅ Agent list with **"＋ New agent"** as the first row → the existing create form.
- ✅ Enter on an agent → live switch (active agent is a no-op notice).
- ✅ `d` delete with a y/n confirm; refused for the active and `default` agents.
  New `Effect::DeleteAgent` → `registry::delete_agent` + list refresh. The list is
  arrow-navigated (no type-filter) so `d` is free for the action.
- ✅ `e` opens an **agent detail/edit overlay**: description, default provider,
  model, dispatch fallbacks (`agent.fallbacks`, now a settable `StrList` key),
  and persona — edited inline (↑/↓ between fields, Enter saves), plus a read-only
  memory-db summary. Loads/saves off the UI task via `LoadAgentDetail` /
  `SaveAgentDetail`; writes config.toml + persona.md + (registry agents) the
  agent.toml description via new `registry::set_agent_description`.
  Note: persona edits here are single-line; rich multi-line persona authoring
  stays in `agent create --persona @file`.

### Phase 3 — Conversations section  ✅
- **"＋ New conversation"** first row → fresh chat; sessions below.
- `e`/F2 inline edit (name + description), `d` delete with confirm, Enter resume.
  The list is arrow-navigated (no type-to-filter) so `e`/`d` are free for actions.
- Net-new backing shipped: `description` on `SessionMetadata`;
  `SessionStore::set_session_description` + `delete_session`; CLI
  `sessions describe` + `sessions delete`. Effects `EditSession` + `DeleteSession`.
- Tests: session-crate round-trip/delete + TUI edit/delete/new-row flows. Green;
  clippy `-D warnings` + fmt clean.

### Phase 4 — Organization section
- List orgs with create-new first row; select (use) an org; detail = users,
  metadata, role.
- **Open decision:** org creation has no codel00p backend route (org lifecycle is
  Clerk's). See "Open decisions" below.

### Phase 5 — Settings section
- Permissions / approvals (set `agent.permission_mode` directly, not only via
  profiles).
- Providers + **API keys** (sensitive — see open decision).
- Display / advanced (exists), appearance/theme (net-new), account settings
  (Clerk-managed, mostly read-only), and the remaining behavior toggles.

### Phase 6 — help/keys + polish
- Update the help overlay and key hints to the new structure; consistency pass.

## Decisions (resolved 2026-06-29)

1. **Build order:** Conversations section first (then Agent, Settings, Organization).
2. **Org creation:** the create-new row **opens the Clerk dashboard** in the
   browser; codel00p stays read-only for org lifecycle. (Phase 4.)
3. **API keys:** the Settings section **edits + stores keys in local
   `config.toml`**. (Phase 5.)
