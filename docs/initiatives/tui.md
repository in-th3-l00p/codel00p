# Initiative 9: Terminal UI (TUI)

## Goal

Give codel00p a rich interactive terminal UI — a live transcript, composer, tool
timeline, and approval surface — for users who live in the terminal but want
more than line-by-line CLI output.

## Status (updated 2026-06-14)

**Phase 2 shipped** as a native **Rust `ratatui` + `crossterm`** TUI in
`core/crates/codel00p-cli/src/tui/` (Elm-style: `app`/`msg`/`update`/`view` +
`event_loop`/`bridge`). Bare `codel00p` (and `codel00p agent chat`) opens it on an
interactive terminal; pipes / CI / `--json-events` fall back to the line REPL.

The **stack decision diverged from the original plan**: we chose Rust ratatui over
Ink + JSON-RPC. The CLI stays self-contained (no Node runtime), but the
"desktop embeds the same TUI" payoff is gone — Phase 3's desktop `/chat` now needs
its own approach (see revised scope). The channel-backed bridge
(`AgentEvent`/token `mpsc` + a oneshot permission policy) is the internal analogue
of the JSON-RPC boundary; a `codel00p agent tui` stdio JSON-RPC mode was **not**
built and is only needed if/when desktop embedding is pursued.

Delivered: streaming transcript, tool-call lifecycle timeline, **inline permission
approval modal**, model picker (F2), org **entity browser** (F3: projects · agents ·
MCP · memory · users · org) with agent-switching, read-only org/role, status bar,
help, and a self-update header chip. ~25 unit tests (pure update/picker/view via
`TestBackend`) + PTY smoke tests. Cloud reads added `CloudClient::list_projects`
/`list_agents`.

## Why (Hermes reference)

Hermes ships a **TUI** (`ui-tui/`, `tui_gateway/`): a React + Ink (TypeScript)
frontend that talks to the Python backend via newline-delimited JSON-RPC over
stdio. It is also embedded in the desktop dashboard's `/chat` page through an
xterm.js PTY bridge, so the same TUI powers both the terminal and the desktop
chat surface — "do not re-implement in React."

## Current codel00p state

- The CLI (`codel00p agent chat`) is an interactive multi-turn loop with
  in-session commands (sessions, history, tools, model switching, memory) and
  live token streaming, but it is line-based stdout, not a full-screen UI.
- The protocol already emits a typed `AgentEvent` stream and `TokenSink` tokens
  — exactly the feed a TUI renders.
- Desktop (Electron) and cloud (Next.js) UIs exist but are early; there is no
  shared interactive transcript component.

## Design

A TUI frontend driven by the existing event/token streams over a stable
stdio JSON-RPC boundary — the same approach as Hermes, and one that lets the
desktop app embed the TUI rather than reimplementing chat in React.

### Transport
- Expose a `codel00p agent tui` (or `--tui`) mode that speaks newline-delimited
  JSON-RPC over stdio: the harness streams `AgentEvent` + tokens out; the UI
  sends user input + control (`approve`/`deny`/`stop`/`switch model`) in.
- This boundary is reusable: the desktop `/chat` page embeds the TUI via an
  xterm.js PTY bridge instead of duplicating chat logic.

### Frontend
- A terminal UI rendering: streaming assistant transcript, a tool-call timeline
  (using the typed events: requested/completed/failed, permission
  requested/decided), token stream, session switcher, and an inline permission
  approval prompt.
- Stack choice (React + Ink to match Hermes and reuse `protocol-ts`, vs a Rust
  TUI like ratatui) is a Phase 1 decision — see open questions.

### Governance fit
- The permission-approval surface is the highest-value TUI feature: render
  `PermissionRequested` events as an inline approve/deny prompt, the terminal
  analogue of the desktop approval UI (Stage 6) and the gateway approval flow
  ([#6](messaging-gateway.md)).

## Scope

### Phase 1 — stack decision — DONE
- [x] Decided stack: **Rust `ratatui` + `crossterm`** (not Ink). CLI stays
      self-contained; trade-off is no shared desktop embed.
- [ ] (Deferred) `codel00p agent tui` stdio JSON-RPC mode — only needed for a
      desktop embed; not built.

### Phase 2 — Core TUI — DONE
- [x] Streaming transcript + tool-call timeline + token stream.
- [x] Inline permission approval modal; model switching (F2).
- [x] Org **entity browser** (F3): projects · agents · MCP · memory, read-only
      org/role; agent selection applies provider+model. (`/sessions /memory
      /history /tools /reset` slash commands; Esc/Ctrl-C handling.)

### Phase 3 — Polish + remaining cloud — NEXT
- [ ] **Writable org switching**: needs a Clerk token re-mint flow in `login.rs`
      (the stored token is scoped to one org); today the Org tab is read-only.
- [ ] **Real Users tab**: needs a backend `GET /org/members` route (Clerk Backend
      API) + an `OrgMember` protocol type + `CloudClient::list_org_members`; today
      the tab shows "backend endpoint pending".
- [ ] Mouse support (click rows / wheel scroll), configurable themes, token/usage
      meters and gauges in the status bar.
- [ ] Source the model picker from a provider `list_models` call instead of the
      hand-maintained catalog in `tui/app.rs`.
- [ ] Session switcher overlay (resume a prior conversation from inside the TUI).

### Phase 4 — Desktop embed (optional, only if pursued)
- [ ] If desktop `/chat` should reuse this TUI, add the stdio JSON-RPC boundary
      and an xterm.js PTY bridge; otherwise the desktop keeps its own chat UI.

## Risks & open questions

- **Stack choice**: Ink reuses `protocol-ts` and enables the desktop embed (the
  Hermes path), but adds a Node runtime dependency to a Rust CLI; a Rust TUI
  keeps the CLI self-contained but cannot be embedded in Electron as easily.
  Recommend Ink for the embed payoff, behind the JSON-RPC boundary so the core
  stays Rust.
- **Scope vs value**: this is an interface nicety, not a capability gap; lower
  priority than the foundation/primitive initiatives. Sequence after the
  governance-relevant work unless a strong terminal-UX demand appears.

## Dependencies

- Consumes the existing protocol event + token streams (no new core capability).
- Shares the approval-surface concept with [#6 Gateway](messaging-gateway.md)
  and Stage 6 desktop interfaces.

## Exit criteria

- A user can run a full-screen codel00p session in the terminal with a live
  transcript, tool timeline, and inline permission approvals, and the desktop
  app reuses the same TUI rather than a separate chat implementation.
