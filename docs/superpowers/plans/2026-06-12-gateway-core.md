# Messaging Gateway Core

First slice of [Initiative 6: Messaging Gateway](../../initiatives/messaging-gateway.md),
Phase 1. The platform-agnostic core: reach one agent from chat, with each
conversation as a durable session.

## Goal

Handle an inbound chat message — map its conversation to a continuous agent
session and run it — so a thread is a remembered conversation. Network adapters
build on this entrypoint.

## Scope

- [x] `codel00p-gateway` crate (dependency-free): `GatewayCommand` +
      `parse_command` (`/help`, `/stop`, `/approve`, `/deny`, else a message),
      `help_text`, and `conversation_session_id` (a stable, id-safe session id
      per conversation — same conversation → same session).
- [x] CLI `codel00p gateway message --conversation <id> --user <id> <text>`:
      control commands answered directly; ordinary text runs as a **read-only**
      agent turn against the conversation's session — Fresh on the first message,
      Resume after (so the thread is remembered). Provider/model from `agent.*`
      config.
- [x] Tests: crate unit tests (command parsing, conversation-id sanitization) and
      `gateway_cli` integration tests — `/help` replies without a provider call;
      two messages in one conversation run turns and **share one session** (the
      transcript of `gateway-c1` contains both).
- [x] Help + dispatch wired. `cargo test`, `cargo fmt --check`, `cargo clippy`.

## Decisions

- **Per-message entrypoint first, transport later.** `gateway message` is exactly
  what a platform adapter (Slack/Telegram/webhook) calls per inbound event. It is
  fully testable with no network, and the conversation→session logic — the core
  value — lives here. An HTTP `gateway serve` and real adapters are thin wrappers
  in the next slice.
- **Conversation = session.** Deriving a stable `gateway-<conversation>` session
  id makes a thread a continuous, resumable agent session with no extra mapping
  store — the session store is the state.
- **Read-only for now.** A remote sender cannot yet approve permissions inline,
  so messages run read-only; the live `/approve` `/deny` flow is the next slice
  (the control commands already parse and are acknowledged).
- **Identity recorded, governance deferred.** `--user` is captured; mapping a
  platform user to a codel00p org/role (Clerk) is a later slice.

## Out of scope (next)

- `gateway serve` (HTTP webhook) and a first real platform adapter (Slack).
- The live permission-ask → chat approval flow, and elevated tool sets behind it.
- Clerk identity resolution + per-org platform/channel allow-list and audit.
- A `PlatformAdapter` trait (added with the first network adapter, to avoid
  speculative abstraction).
