# Initiative 6: Messaging Gateway

## Goal

Let one codel00p agent core be reached from the platforms teams already live in
— Slack, Telegram, Discord, Teams, email, and more — instead of only the CLI,
desktop, and cloud UI.

## Why (Hermes reference)

"Lives where you do" is a core Hermes pitch: the same agent core runs across CLI,
TUI, Electron, and a **20+ platform messaging gateway** (Telegram, Discord,
Slack, WhatsApp, Signal, Matrix, Mattermost, Microsoft Teams, Google Chat,
DingTalk, Feishu, WeCom, Email, SMS, and more) from one gateway.

Architecture (`gateway/run.py`, `gateway/platforms/`): platform adapters share a
single agent core; session state is managed per-platform; the base adapter
queues messages when an agent is busy; control commands (`/stop`, `/approve`,
`/deny`) bypass message guards to reach the running agent.

## Current codel00p state

- Surfaces are CLI, Electron desktop, and the Next.js cloud UI talking to the
  axum `codel00p-cloud` backend (`GET /me`, `GET/POST /projects`).
- No chat-platform delivery, no inbound message handling, no per-conversation
  session mapping for external platforms.
- `codel00p-session` durable sessions + the protocol event stream are the right
  substrate; the gateway needs a per-platform session-id mapping and an inbound
  command surface.

## Design

A new `codel00p-gateway` service that hosts platform adapters over the shared
harness, plus an adapter trait so platforms are pluggable
([#1](plugins-and-hooks.md)).

### Adapter trait
- `PlatformAdapter`: `receive` (inbound message -> normalized event),
  `send` (agent output -> platform message), `controls` (map `/stop`,
  `/approve`, `/deny`, `/model` to agent control), and identity mapping
  (platform user/channel -> codel00p `user_id` + `session_id`).
- Adapters ship as plugins so the core ships thin and orgs enable only what they
  use.

### Session mapping
- Each platform conversation maps to a durable `codel00p-session` (per-platform,
  per-channel, per-user). Reuse session resume so a Slack thread is a continuous
  agent session.
- Base adapter queues inbound messages while a turn is active; control commands
  bypass the queue to reach the running agent (mirrors Hermes).

### Permissions & approvals
- The existing `PermissionMode::Ask` flow maps naturally to chat: a permission
  request becomes a message with `/approve` `/deny` buttons/commands. This is
  the gateway's most valuable governance feature — human-in-the-loop approval
  from where the team already works.

### Auth & governance (codel00p-specific)
- Platform identity must resolve to a Clerk-backed `user_id` within an org, so
  gateway access honors the same org/role model as the cloud control plane. A
  Slack user with no linked codel00p identity gets no agent access.
- Per-org allowlist of enabled platforms + channels; audit every gateway-driven
  action like any other session.

### Hosting
- Runs alongside or inside `codel00p-cloud` (axum) for hosted teams; a local
  `codel00p gateway` mode for self-hosted/personal use.

## Scope

### Phase 1 — Adapter framework + one platform
- [x] Gateway core: `codel00p-gateway` crate (control-command parsing +
      conversation→session derivation) and `codel00p gateway message
      --conversation <id> --user <id> <text>` — the per-message entrypoint a
      platform adapter calls. Each conversation maps to a durable, resumable
      agent session (a thread is remembered). Slice:
      [2026-06-12-gateway-core](../superpowers/plans/2026-06-12-gateway-core.md).
- [x] `/help`, `/stop`, `/approve`, `/deny` control commands handled before the
      agent runs (approval is acknowledged; the live approval flow is below).
- [x] HTTP webhook: `codel00p gateway serve` — a minimal, dependency-free server
      (`POST /message {conversation,user,text}` -> `{reply}`, `GET /healthz`) that
      platform event subscriptions post to. Slice:
      [2026-06-12-gateway-serve](../superpowers/plans/2026-06-12-gateway-serve.md).
- [ ] A first real platform adapter (Slack), inbound/outbound, translating
      platform events to `POST /message`.
- [ ] Live permission-ask → chat `/approve` `/deny` flow (today messages run
      read-only).

### Phase 2 — Governance integration
- [ ] Clerk identity resolution for platform users; org/role enforcement.
- [ ] Permission-ask -> chat approval flow.
- [ ] Per-org platform/channel allowlist + audit.

### Phase 3 — Breadth
- [ ] Additional adapters as plugins (Telegram, Discord, Teams, email),
      prioritized by demand.
- [ ] Scheduled-job delivery into platforms ([#5](scheduling-cron.md)).

## Risks & open questions

- **Scope creep**: 20+ platforms is a long tail; ship the adapter framework +
  Slack first, let the rest be community/plugin-driven.
- **Identity mapping**: the hard part is binding a platform user to a governed
  codel00p identity safely; do not allow anonymous agent access.
- **Concurrency & rate limits**: per-platform queues and backpressure; one busy
  channel must not stall others.
- **Secrets**: platform tokens are secrets — store in the existing `.env`/secret
  path, never config.

## Dependencies

- [#1 Plugins & Hooks](plugins-and-hooks.md) (adapters as plugins).
- `codel00p-cloud` Clerk identity for governed access.
- Pairs with [#5 Scheduling](scheduling-cron.md) for proactive delivery.

## Exit criteria

- A team member can hold a continuous, governed, auditable codel00p session from
  Slack — including approving tool permissions inline — with their identity
  resolved to their codel00p org role, and new platforms can be added as plugins.
