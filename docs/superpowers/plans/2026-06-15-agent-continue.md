# Slice: `agent continue` — resume your most recent session

Date: 2026-06-15
Roadmap: Milestone 1 (Local Agent Foundation) — "most-recent session continue
command". Priority order #1: make the CLI more production-useful today.

## Goal

`codel00p agent continue <prompt>` resumes the most recently created session
without the user having to look up its id — the codel00p analogue of Claude
Code's `--continue`/`-c`.

## Why this slice

`agent resume` already exists but demands an explicit `<session-id>`. The common
case is "keep going where I left off." It is small, self-contained, and fully
testable, and ranks above interface polish in the loop's priority order.

## The ordering problem

`SessionMetadata` carries no creation time, session ids mix schemes (`session-N`
process counter vs `chat-{nanos}`), and `list_sessions()` orders by id string —
so there is no reliable "most recent" today. This slice adds the missing signal.

## Design

1. **`codel00p-session`**: `SessionMetadata` gains `created_at: Option<u64>`
   (unix epoch millis), a `with_created_at` builder, and a `created_at()`
   accessor. `new()` leaves it `None`; serde `default` + `skip_serializing_if`
   keeps existing persisted sessions and golden output unchanged.
2. **CLI persistence**: `persist_session_records` stamps `created_at` at session
   creation (the one `create_session` call), so every new session is dated.
3. **Selector** (pure, unit-tested): `latest_session(&[SessionMetadata]) ->
   Option<&SessionMetadata>` picks the max `created_at` (undated sessions sort
   oldest; ties break by id) — deterministic without wall-clock in tests.
4. **Command**: `agent continue <prompt> [options]` resolves the latest session
   id, sets it on the parsed run options, and runs in `Resume` mode (reusing the
   existing resume path). Clear error when there is no session to continue.
5. **Help + docs**: agent help lists `continue`; roadmap "Next" updated.

## Tests (test-first)

- session crate: `created_at` round-trips; default `None`; legacy JSON without
  the field deserializes.
- CLI: `latest_session` picks the newest, tie-breaks, tolerates empty / all-None.
- CLI: `agent continue` with no sessions returns a clear error.

## Out of scope

Newest-first ordering of `session list` output (a trivial companion follow-up),
and any change to the `session-N` id scheme.
