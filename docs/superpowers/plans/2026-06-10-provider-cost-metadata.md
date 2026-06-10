# Provider Cost Metadata Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add deterministic, provider-neutral usage cost estimates to inference responses when pricing is supplied by the caller.

**Architecture:** Cost estimation stays explicit and local to the inference request. The provider crate will not hard-code vendor prices; callers can attach `UsagePricing` to an `InferenceRequest`, and `InferenceClient::complete` will derive a normalized `UsageCostEstimate` from the response usage counters. Transports remain responsible only for usage normalization.

**Tech Stack:** Rust, `codel00p-providers`, `serde`, mocked HTTP tests, `cargo test`, `cargo clippy`, `pnpm verify`.

---

### Task 1: Add Failing Cost Interface Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/client_interface.rs`
- Create: `core/crates/codel00p-providers/tests/usage_cost.rs`

- [ ] **Step 1: Test request pricing shape**

Add a test that builds an `InferenceRequest` with:

```rust
UsagePricing::usd_nanos_per_million_tokens(150_000_000, 600_000_000)
    .with_cache_read_nanos_per_million_tokens(50_000_000)
    .with_cache_write_nanos_per_million_tokens(75_000_000)
    .with_reasoning_nanos_per_million_tokens(600_000_000)
```

Assert the request stores the pricing object exactly and keeps provider/model
fields unchanged.

- [ ] **Step 2: Test deterministic cost math**

Add a pure unit test that estimates cost for:

```rust
Usage {
    input_tokens: 7,
    output_tokens: 4,
    cache_read_tokens: 3,
    cache_write_tokens: 2,
    reasoning_tokens: 1,
}
```

Expected component costs in nano-USD:

- input: `1050`
- output: `2400`
- cache read: `150`
- cache write: `150`
- reasoning: `600`
- total: `4350`

- [ ] **Step 3: Test response cost attachment**

Use mocked Chat Completions with a response usage payload. Build an
`InferenceClient`, attach pricing to the request, and assert the returned
`InferenceResponse.cost` contains the normalized estimate.

- [ ] **Step 4: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test client_interface request_builder_preserves_usage_pricing
cd core && cargo test -p codel00p-providers --test usage_cost
```

Expected: compile failure because `UsagePricing` and `InferenceResponse.cost`
do not exist.

### Task 2: Implement Cost Types

**Files:**
- Modify: `core/crates/codel00p-providers/src/response.rs`
- Modify: `core/crates/codel00p-providers/src/request.rs`
- Modify: `core/crates/codel00p-providers/src/lib.rs`

- [ ] **Step 1: Add `UsagePricing`**

Add a public serializable pricing struct:

```rust
pub struct UsagePricing {
    pub currency: String,
    pub input_nanos_per_million_tokens: u64,
    pub output_nanos_per_million_tokens: u64,
    pub cache_read_nanos_per_million_tokens: u64,
    pub cache_write_nanos_per_million_tokens: u64,
    pub reasoning_nanos_per_million_tokens: u64,
}
```

Add builder helpers for USD and optional cache/reasoning rates.

- [ ] **Step 2: Add `UsageCostEstimate`**

Add a public serializable estimate struct:

```rust
pub struct UsageCostEstimate {
    pub currency: String,
    pub input_nanos: u64,
    pub output_nanos: u64,
    pub cache_read_nanos: u64,
    pub cache_write_nanos: u64,
    pub reasoning_nanos: u64,
    pub total_nanos: u64,
}
```

Add `Usage::estimate_cost(&UsagePricing) -> UsageCostEstimate`.

- [ ] **Step 3: Extend request and response structs**

Add:

```rust
pub pricing: Option<UsagePricing>
```

to `InferenceRequest`, plus `InferenceRequestBuilder::pricing(...)`.

Add:

```rust
pub cost: Option<UsageCostEstimate>
```

to `InferenceResponse`.

### Task 3: Wire Cost Estimation Into Client Completion

**Files:**
- Modify: `core/crates/codel00p-providers/src/client.rs`
- Update transport response literals if required by the new response field.

- [ ] **Step 1: Attach estimates after successful primary route**

After a successful single-route completion, if both `request.pricing` and
`response.usage` are present, set `response.cost`.

- [ ] **Step 2: Attach estimates after successful fallback route**

When a fallback route succeeds, use the fallback request pricing inherited from
the original request and set `response.cost` before returning.

- [ ] **Step 3: Leave unknown costs explicit**

If pricing is absent or provider usage is absent, leave `response.cost` as
`None`.

### Task 4: Document, Verify, Commit, Push

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/agentic-backlog.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Document the new public API**

Mention explicit request pricing and normalized response cost estimates.

- [ ] **Step 2: Run targeted provider checks**

Run:

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test client_interface request_builder_preserves_usage_pricing && cargo test -p codel00p-providers --test usage_cost && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [ ] **Step 3: Run repo verification**

Run:

```bash
pnpm verify
```

- [ ] **Step 4: Commit and push**

Run:

```bash
git add core/crates/codel00p-providers/src/response.rs core/crates/codel00p-providers/src/request.rs core/crates/codel00p-providers/src/lib.rs core/crates/codel00p-providers/src/client.rs core/crates/codel00p-providers/tests/client_interface.rs core/crates/codel00p-providers/tests/usage_cost.rs core/crates/codel00p-providers/README.md docs/providers.md docs/agentic-backlog.md docs/product-roadmap.md docs/roadmap.md docs/superpowers/plans/2026-06-10-provider-cost-metadata.md
git commit -m "feat: add provider cost metadata"
git push origin main
```
