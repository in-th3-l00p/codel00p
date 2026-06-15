# Slice: Retry-with-backoff per route (honor Retry-After)

Date: 2026-06-15
Roadmap: Provider Route Intelligence (backlog #1) — resilient routing.

## Goal

Make the inference client retry transient provider failures (rate limits,
overload) on the same route before falling back, with state-of-the-art backoff:
exponential delay + full jitter, honoring a provider `Retry-After` hint, capped
by a max delay and a bounded attempt count.

## Why this slice

Builds directly on the captured `Retry-After` metadata: the client previously
tried each route exactly once, so a brief 429 immediately burned the route or
fell back. Bounded retry-with-backoff is the standard resilient-client behavior
and is fully testable offline.

## Design

- **`client/retry.rs` — `RetryPolicy`** (pure, exported): `max_retries`,
  `base_delay`, `max_delay`. `should_retry(attempt, retryable)` bounds attempts;
  `delay(attempt, retry_after, jitter)` returns the `Retry-After` hint (capped)
  when present, else `base * 2^attempt` capped at `max_delay`, scaled by a jitter
  fraction in `[0,1]`. `default()` = 2 retries / 500 ms / 20 s cap;
  `disabled()` = one attempt.
- **`client.rs`**: `InferenceClient` gains a `RetryPolicy`;
  `complete_one_with_retry` loops `complete_one`, sleeping `policy.delay(...)`
  (tokio `time`) on retryable errors until the bound is hit, then returns the
  last failure so `complete` can fall back. Both the primary and each fallback
  route go through it. A wall-clock-derived `jitter_fraction()` avoids an RNG
  dependency.
- **`builder.rs`**: `retry_policy(RetryPolicy)` setter; defaults to
  `RetryPolicy::default()`.

## Tests (offline)

- `retry.rs` units: bound + retryability, exponential doubling + cap, jitter
  scaling/clamping, `Retry-After` wins + cap.
- `tests/retry_backoff.rs`: a 429 with `Retry-After: 0` is retried `1 + max`
  times (asserted via httpmock call counts) then falls back; the hint shows up as
  `retry_after_secs` in attempt metadata; a 401 is never retried.
- `tests/fallback_routing.rs`: the rate-limit→fallback test now sets
  `RetryPolicy::disabled()` to isolate fallback from retry.

## Out of scope

The HTTP-date form of `Retry-After` (providers use integer seconds for 429s); a
per-request retry override on `InferenceRequest`; surfacing retry counts in
metadata. Follow-ups.
