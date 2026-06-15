# Slice: cooperative turn cancellation (Ctrl-C)

Date: 2026-06-15
Roadmap: Milestone 1 (Local Agent Foundation) — "cancellation and interruption".
Priority order #1: make the CLI/harness more production-useful today.

## Goal

Let a user stop a running agent turn with Ctrl-C and keep the work done so far,
instead of killing the process and losing the session — the codel00p analogue of
interrupting Claude Code mid-turn.

## Design

A cooperative cancellation signal checked at turn boundaries, so a stop is clean
and the partial session still persists.

1. **`CancelSignal`** (`codel00p-harness::cancel`): a cheap `Arc<AtomicBool>`
   handle — `new`/`cancel`/`is_cancelled`, `Clone`, `Default` (never cancelled).
2. **Harness**: `AgentHarness` holds a `CancelSignal` (default never-cancelled);
   `AgentHarnessBuilder::cancel_signal` wires one in.
3. **Run loop** (`agent/turn.rs`): check `is_cancelled()` at the top of each
   iteration and again after inference before executing a tool batch. On cancel,
   emit `TurnCompleted` and return `Ok(TurnOutcome { cancelled: true, .. })` with
   whatever messages/tool results accumulated — no error, so the caller persists.
4. **`TurnOutcome.cancelled`**: a new bool (harness-only struct, not a wire
   contract) so the caller can report the interruption.
5. **CLI** (`run_agent_turn`): spawn a `tokio::signal::ctrl_c` watcher that flips
   the signal; pass it to the harness. On a cancelled outcome, print a clear
   "Interrupted — partial progress saved." line; the session persists as usual.

## Tests (test-first)

- harness: a pre-cancelled signal makes `run_turn` return `cancelled = true` with
  zero inference calls (mock client counts calls) and no assistant message.
- harness: cancelling after the first inference (a model client that trips the
  signal) stops before a second inference and returns `cancelled = true`.
- `CancelSignal` unit: starts false, `cancel()` latches true, clones share state.

## Out of scope (follow-ups)

Mid-inference / mid-tool abort (racing the future against the signal), and the
TUI's Esc-to-interrupt wiring — both build on this primitive.
