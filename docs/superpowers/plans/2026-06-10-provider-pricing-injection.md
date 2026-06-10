# Provider Pricing Injection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let organizations inject provider/model pricing into `InferenceClient` so requests do not need to carry pricing every time.

**Architecture:** Keep request-level `InferenceRequest::pricing(...)` as the highest-priority override. Add client-level provider/model pricing on `InferenceClientBuilder`, canonicalize provider aliases at build time, and use the selected route provider/model when attaching usage cost estimates.

**Tech Stack:** Rust, `codel00p-providers`, mocked Chat Completions tests, deterministic nano-unit cost math.

---

### Task 1: Add Red Pricing Injection Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/usage_cost.rs`

- [ ] **Step 1: Test client model pricing**

Create a mocked `custom` completion response with usage but no request pricing. Build the client with:

```rust
.model_pricing(
    "custom",
    "test-model",
    UsagePricing::usd_nanos_per_million_tokens(150_000_000, 600_000_000),
)
```

Assert the response gets a `UsageCostEstimate`.

- [ ] **Step 2: Test request pricing wins**

Create a client with one model pricing entry and a request with different `.pricing(...)`. Assert the resulting cost uses request pricing.

- [ ] **Step 3: Test provider aliases are canonicalized**

Register pricing for alias `ollama`, request canonical provider `custom`, and assert the cost is attached.

- [ ] **Step 4: Run tests to verify failure**

Run: `cargo test -p codel00p-providers --test usage_cost`
Expected: FAIL because `InferenceClientBuilder::model_pricing` does not exist.

### Task 2: Implement Client Pricing Catalog

**Files:**
- Modify: `core/crates/codel00p-providers/src/client.rs`

- [ ] **Step 1: Add pricing storage**

Add `model_pricing: BTreeMap<String, BTreeMap<String, UsagePricing>>` to `InferenceClient` and `InferenceClientBuilder`.

- [ ] **Step 2: Add builder method**

Add:

```rust
pub fn model_pricing(
    mut self,
    provider: impl Into<String>,
    model: impl Into<String>,
    pricing: UsagePricing,
) -> Self
```

- [ ] **Step 3: Canonicalize provider keys**

During builder `build()`, canonicalize model pricing provider keys through the registry, matching credential and policy behavior.

- [ ] **Step 4: Attach cost from selected route**

Change cost attachment to use request pricing first, then client model pricing for the selected route provider/model. For fallback success, use the fallback provider/model instead of the primary request model.

- [ ] **Step 5: Run tests to verify pass**

Run: `cargo test -p codel00p-providers --test usage_cost`
Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/product-roadmap.md`

- [ ] **Step 1: Document pricing injection**

Explain request pricing precedence and client-level provider/model pricing.

- [ ] **Step 2: Run targeted verification**

Run:

```bash
cargo fmt --all
cargo test -p codel00p-providers --test usage_cost
cargo test -p codel00p-providers
cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

Expected: all commands pass.

- [ ] **Step 3: Run repo verification**

Run: `pnpm verify`
Expected: PASS.

- [ ] **Step 4: Commit and push**

Run:

```bash
git add docs/superpowers/plans/2026-06-10-provider-pricing-injection.md core/crates/codel00p-providers docs
git commit -m "feat: add provider pricing injection"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
