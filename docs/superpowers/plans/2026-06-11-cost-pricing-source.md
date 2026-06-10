# Cost Pricing Source Metadata Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use test-driven-development before implementation. Keep this additive and provider-neutral.

**Goal:** Make response cost estimates auditable by recording whether pricing came from the request, configured client pricing, or a published pricing catalog.

**Architecture:** Add optional `pricing_source` metadata to `UsageCostEstimate`. `Usage::estimate_cost(...)` remains pure math and leaves the source empty. `InferenceClient` attaches the source after selecting request pricing or stored client/catalog pricing. `ProviderPricingCatalog` gains optional source metadata so published catalogs can identify the table that supplied prices.

**Tech Stack:** Rust, `codel00p-providers`, mocked usage-cost tests, Markdown docs, `pnpm verify`.

---

### Task 1: Add Failing Pricing Source Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/usage_cost.rs`

- [x] **Step 1: Write request pricing source assertion**

Assert a response priced from `InferenceRequest::pricing(...)` has `cost.pricing_source == Some("request")`.

- [x] **Step 2: Write configured model pricing source assertion**

Assert `.model_pricing(...)` produces `cost.pricing_source == Some("configured")`.

- [x] **Step 3: Write published catalog source test**

Create a `ProviderPricingCatalog::new(...).with_source("catalog:team-ai-2026-06")`, pass it to `.pricing_catalog(...)`, and assert the response cost carries that source.

- [x] **Step 4: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test usage_cost
```

Expected: compile failures because pricing source metadata does not exist yet.

### Task 2: Implement Pricing Source Metadata

**Files:**
- Modify: `core/crates/codel00p-providers/src/response.rs`
- Modify: `core/crates/codel00p-providers/src/pricing_catalog.rs`
- Modify: `core/crates/codel00p-providers/src/client.rs`

- [x] **Step 1: Add optional cost source field**

Add `pricing_source: Option<String>` to `UsageCostEstimate` with serde defaults and skip-empty serialization.

- [x] **Step 2: Add catalog source metadata**

Add optional `source` to `ProviderPricingCatalog` plus `with_source(...)`.

- [x] **Step 3: Track stored pricing sources**

Store configured model pricing with source `configured` and catalog pricing with the catalog source or default `catalog`.

- [x] **Step 4: Attach source after cost estimation**

Set `pricing_source` on response costs from request, configured, and catalog pricing paths.

### Task 3: Document Pricing Sources

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Update provider docs**

Document pricing source metadata for request, configured, and catalog pricing.

- [x] **Step 2: Update backlog status**

Mark usage/cost metadata as including pricing source labels.

### Task 4: Verify, Commit, Push

- [x] **Step 1: Run focused checks**

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test usage_cost && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Step 2: Run repo verification**

```bash
pnpm verify
```

- [x] **Step 3: Commit and push**

Commit as `feat: add cost pricing source metadata` and push to `origin/main`.
