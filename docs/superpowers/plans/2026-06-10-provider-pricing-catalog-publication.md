# Provider Pricing Catalog Publication Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let cloud or organization services publish a provider/model pricing catalog that can be loaded into `InferenceClientBuilder` without every request carrying pricing.

**Architecture:** Keep `UsagePricing` as the single pricing math type. Add a small serializable `ProviderPricingCatalog` made of provider/model entries, then ingest that catalog into the existing client-level pricing map. Builder call order remains explicit: later pricing entries for the same canonical provider/model replace earlier entries.

**Tech Stack:** Rust, serde, `codel00p-providers`, mocked Chat Completions usage tests.

---

### Task 1: Add Red Pricing Catalog Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/usage_cost.rs`

- [ ] **Step 1: Test published catalog JSON shape**

Deserialize a `ProviderPricingCatalog` from JSON with one provider/model entry
and assert the nested `UsagePricing` values are preserved.

- [ ] **Step 2: Test catalog ingestion**

Build an `InferenceClient` with `.pricing_catalog(catalog)`, make a mocked
completion request with usage and no request-level pricing, and assert cost is
estimated from the catalog.

- [ ] **Step 3: Test provider aliases are canonicalized**

Use an alias such as `ollama` in the catalog and request provider `custom`;
assert cost is still estimated.

- [ ] **Step 4: Run tests to verify failure**

Run: `cargo test -p codel00p-providers --test usage_cost`

Expected: FAIL because `ProviderPricingCatalog` and
`InferenceClientBuilder::pricing_catalog` do not exist.

### Task 2: Implement Pricing Catalog Types

**Files:**
- Create: `core/crates/codel00p-providers/src/pricing_catalog.rs`
- Modify: `core/crates/codel00p-providers/src/lib.rs`
- Modify: `core/crates/codel00p-providers/src/client.rs`

- [ ] **Step 1: Add serializable catalog structs**

Add `ProviderPricingCatalog` and `ProviderModelPricing` with an `entries` array
of provider/model/pricing rows.

- [ ] **Step 2: Add ergonomic constructors**

Add `ProviderPricingCatalog::new(...)` and `ProviderModelPricing::new(...)` for
tests and callers that construct catalogs directly.

- [ ] **Step 3: Re-export the catalog types**

Re-export the new types from `codel00p-providers`.

- [ ] **Step 4: Add builder ingestion**

Add:

```rust
pub fn pricing_catalog(mut self, catalog: ProviderPricingCatalog) -> Self
```

Insert each catalog entry into the existing provider/model pricing map.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/product-roadmap.md`

- [ ] **Step 1: Document pricing catalog publication**

Explain that cloud/organization services can publish provider/model pricing
catalogs and that request-level pricing still wins at inference time.

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
git add docs/superpowers/plans/2026-06-10-provider-pricing-catalog-publication.md core/crates/codel00p-providers docs
git commit -m "feat: add provider pricing catalogs"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
