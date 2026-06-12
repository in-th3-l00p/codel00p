# Gateway HTTP Webhook (`gateway serve`)

Second slice of [Initiative 6: Messaging Gateway](../../initiatives/messaging-gateway.md),
Phase 1. A running endpoint that platform event subscriptions post to.

## Goal

Expose the per-message gateway over HTTP, so Slack/Telegram/Discord webhooks (or
anything) can drive the agent with no codel00p-specific client.

## Scope

- [x] `codel00p gateway serve [--bind <addr>] [--port <n>]` (default
      `127.0.0.1:8765`): a minimal, **dependency-free** HTTP/1.1 server.
  - `POST /message` with `{conversation,user,text}` -> `{reply}` (runs the same
    `run_gateway_message` as the CLI: control commands + a read-only agent turn
    on the conversation's durable session).
  - `GET /healthz` -> `{status:"ok"}`; unknown routes -> 404; invalid JSON -> 400.
  - Sequential (one request at a time) so turns for a conversation never race on
    its session.
- [x] Tests: a `dispatch` unit test (health / 404 / bad-JSON) and a **real
      same-process HTTP e2e** — bind port 0, run the accept loop in a thread, and
      drive `GET /healthz` and `POST /message` over an actual socket against a
      mock provider, asserting the agent's reply in the response body.
- [x] Help updated. `cargo test`, `cargo fmt --check`, `cargo clippy`.

## Decisions

- **Hand-rolled minimal server, no web framework.** A webhook needs only: read
  the request line + `Content-Length`, read the body, route, write a JSON
  response. That is ~80 lines of `std::net`, keeps the CLI binary lean, and
  builds offline (no axum/tiny_http). `serve_loop` is `pub(crate)` so it can be
  driven over a real socket in-process for a flake-free e2e.
- **Sequential handling.** Agent turns are not cheap and must not race on a
  conversation's session, so requests are handled one at a time. A bounded worker
  pool (keyed by conversation) can come later if throughput needs it.
- **Same core as the CLI.** `serve` and `gateway message` both call
  `run_gateway_message`, so behaviour (sessions, control commands, read-only
  execution) is identical across transports.

## Out of scope (next)

- A first real platform adapter (Slack): verify the signing secret, translate
  events to `POST /message`, post replies back.
- The live permission-ask -> chat `/approve` `/deny` flow and elevated tool sets.
- Auth on the webhook (shared secret / Clerk) and per-org channel allow-list.
- Concurrency (per-conversation worker pool) and TLS (front with a proxy today).
