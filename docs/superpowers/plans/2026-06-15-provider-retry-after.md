# Slice: Honor provider `Retry-After` in route intelligence

Date: 2026-06-15
Roadmap: Provider Route Intelligence (backlog #1) — route audit metadata.

## Goal

When a provider rate-limits or sheds load with a `Retry-After` header, capture
the suggested backoff so the error classifier and the route-audit metadata can
surface it (the foundation for honoring it in retry/backoff).

## Why this slice

It is the smallest increment to "route audit metadata" that is fully testable
without live credentials: pure header parsing + message classification +
JSON surfacing, wired end to end through every transport.

## Design

1. **Transports** (`transports/mod.rs`): two shared helpers —
   `retry_after_seconds(headers)` (integer-seconds form; HTTP-date → `None`) and
   `http_status_error(provider, status, body, retry_after)` which folds the hint
   into the error message as `status <code> (retry-after: <n>s): <body>`. All
   eight non-success sites (chat completions, azure, responses, gemini,
   anthropic, sse, bedrock ×2) now use them; messages without a hint are
   byte-for-byte unchanged.
2. **Classifier** (`error_classifier.rs`): `ClassifiedProviderError` gains
   `retry_after_secs: Option<u64>` + `with_retry_after` + accessor;
   `classify_message` parses the hint and attaches it on the retryable kinds
   (rate limit, transient unavailability).
3. **Route metadata** (`client/metadata.rs`): `failed_attempt` writes
   `retry_after_secs` into the attempt JSON beside `error_kind`, so it appears in
   `codel00p_route.attempts[]`.

## Tests (offline)

- `transports`: header parse (integer / absent / HTTP-date), message folding.
- `error_classifier`: hint parse, rate-limit + unavailable carry the hint,
  no-hint stays `None`.

## Out of scope

Actually sleeping for the suggested delay in the retry loop, and the HTTP-date
form of `Retry-After` — follow-ups once a backoff policy lands.
