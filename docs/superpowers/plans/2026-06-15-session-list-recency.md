# Slice: recency-ordered session listing

Date: 2026-06-15
Roadmap: Milestone 1 — companion to `agent continue` (the deferred follow-up).

## Goal

Show saved sessions most-recent-first instead of in id order, so `session list`
and the chat sessions overlay surface the conversation you most likely want.

## Why

`agent continue` added a `created_at` on session metadata but the listings still
ordered by id string (meaningless: `session-10` before `session-2`,
`chat-{nanos}` interleaved). Sorting by recency is the natural, expected view and
reuses the timestamp already stored.

## Design

- `session list`: carry `created_at` into `SessionSummary`, sort newest-first
  (`created_at` DESC, undated/legacy sessions last, ties broken by id), and add
  `created_at` to `--json` output.
- `chat_sessions_listing`: same recency sort for the in-chat `/sessions` overlay.

## Tests

- `session list` prints newest-first (a newer session with a later `created_at`
  sorts above an id-earlier one).
- undated (pre-timestamp) sessions sort after dated ones.
- `--json` reports `created_at`.

## Out of scope

A human-readable age/timestamp column in the text output (kept the existing
format to avoid churn; the value is in `--json`).
