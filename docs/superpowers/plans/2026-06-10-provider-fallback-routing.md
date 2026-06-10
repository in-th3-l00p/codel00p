# Provider Fallback Routing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add explicit provider fallback routing for retryable/provider-unavailable failures while preserving deterministic audit metadata.

**Architecture:** Fallback is opt-in per `InferenceRequest`. A request keeps its messages, tools, temperature, and output-token options while fallback candidates override provider, model, and optional base URL. `InferenceClient::complete` uses the existing provider error classifier and only advances to a fallback when `should_fallback()` is true.

**Tech Stack:** Rust, `codel00p-providers`, `httpmock`, `serde_json`, provider tests, `cargo test`, `cargo clippy`, `pnpm verify`.

---

### Task 1: Add Fallback Request And Client Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/client_interface.rs`
- Create: `core/crates/codel00p-providers/tests/fallback_routing.rs`

- [x] **Step 1: Write failing request-shape test**

Assert `InferenceRequest::builder(...).fallback_route(...)` stores provider,
model, and optional base URL without changing the primary provider.

- [x] **Step 2: Write failing fallback behavior tests**

Add mocked HTTP tests that prove:

- a 429 primary response falls back to the next configured route;
- a non-fallbackable auth error does not call the fallback route;
- a successful fallback response includes `codel00p_route` provider data with
  selected provider/model and ordered attempts.

- [x] **Step 3: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test client_interface request_builder_preserves_fallback_routes
cd core && cargo test -p codel00p-providers --test fallback_routing
```

Expected: compile failure because fallback routes are not implemented yet.

### Task 2: Implement Fallback Route Request Shape

**Files:**
- Modify: `core/crates/codel00p-providers/src/request.rs`
- Modify: `core/crates/codel00p-providers/src/lib.rs`

- [x] **Step 1: Add `InferenceFallbackRoute`**

Add a public route struct with provider, model, optional base URL, and builder
helpers.

- [x] **Step 2: Extend `InferenceRequest` and builder**

Add `fallback_routes: Vec<InferenceFallbackRoute>` and
`InferenceRequestBuilder::fallback_route(...)`.

### Task 3: Implement Client Fallback Execution

**Files:**
- Modify: `core/crates/codel00p-providers/src/client.rs`

- [x] **Step 1: Split single-route execution**

Extract a helper that resolves a request, fetches credentials, dispatches the
transport, and returns the route plus result.

- [x] **Step 2: Add fallback loop**

Try the primary route first. On an error classified with `should_fallback()`,
try fallback candidates in order. Stop immediately for non-fallbackable errors.

- [x] **Step 3: Add safe route attempt metadata**

On successful completion, attach `codel00p_route` to `provider_data`, including
selected provider/model and ordered attempts with provider, model, outcome, and
error kind when present.

### Task 4: Verify, Commit, Push

**Files:**
- All files above.

- [x] **Step 1: Run targeted provider checks**

Run:

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test client_interface request_builder_preserves_fallback_routes && cargo test -p codel00p-providers --test fallback_routing && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Step 2: Run repo verification**

Run:

```bash
pnpm verify
```

- [x] **Step 3: Commit and push**

Run:

```bash
git add core/crates/codel00p-providers/src/request.rs core/crates/codel00p-providers/src/lib.rs core/crates/codel00p-providers/src/client.rs core/crates/codel00p-providers/tests/client_interface.rs core/crates/codel00p-providers/tests/fallback_routing.rs docs/superpowers/plans/2026-06-10-provider-fallback-routing.md
git commit -m "feat: add provider fallback routing"
git push origin main
```
