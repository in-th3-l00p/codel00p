# Provider Catalog Policy Metadata Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use test-driven-development before editing implementation code. Keep this slice additive and independently shippable.

**Goal:** Make model catalog policy filtering auditable without changing the existing `list_models()` convenience API.

**Architecture:** Add a richer catalog response type that wraps the normalized models with safe metadata: requested provider, canonical provider, catalog URL, policy decision, active model policy filters, catalog model count, and returned model count. `InferenceClient::list_models()` will delegate to the richer method and return only models for compatibility.

**Tech Stack:** Rust, `codel00p-providers`, mocked HTTP catalog tests, Markdown docs, `pnpm verify`.

---

### Task 1: Add Failing Catalog Metadata Test

**Files:**
- Modify: `core/crates/codel00p-providers/tests/model_catalog.rs`

- [x] **Step 1: Write the test**

Add a mocked catalog test that calls the richer API with both allowed-model and required-capability policy filters. Assert:

- requested provider and canonical provider are reported;
- catalog URL is reported;
- policy decision is `Allowed`;
- policy filters report the allowed models and required capabilities;
- catalog model count reflects the provider payload before filtering;
- returned model count and models reflect policy filtering.

- [x] **Step 2: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test model_catalog list_model_catalog_reports_policy_metadata
```

Expected: compile failure because the richer catalog API and metadata types do not exist yet.

### Task 2: Implement Additive Catalog Metadata API

**Files:**
- Modify: `core/crates/codel00p-providers/src/model_catalog.rs`
- Modify: `core/crates/codel00p-providers/src/policy.rs`
- Modify: `core/crates/codel00p-providers/src/client.rs`
- Modify: `core/crates/codel00p-providers/src/lib.rs`
- Modify: `core/crates/codel00p-providers/src/runtime.rs`

- [x] **Step 1: Add public metadata structs**

Add `ProviderModelCatalog` and `ProviderModelCatalogPolicy` with serde support.

- [x] **Step 2: Expose policy filter metadata**

Add an internal policy helper that returns allowed models and required capability filters for a canonical provider.

- [x] **Step 3: Add `InferenceClient::list_model_catalog`**

Reuse existing catalog fetching/parsing, apply policy filtering, and return the richer metadata.

- [x] **Step 4: Preserve `list_models()`**

Make `list_models()` delegate to `list_model_catalog()` and return `catalog.models`.

### Task 3: Document The Catalog Metadata

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Update provider docs**

Document that catalog listing now has an auditable response form for policy-aware UI and cloud control surfaces.

- [x] **Step 2: Update backlog status**

Mark provider/model allowlist policy hooks as including catalog policy metadata.

### Task 4: Verify, Commit, Push

- [x] **Step 1: Run focused checks**

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test model_catalog list_model_catalog_reports_policy_metadata && cargo test -p codel00p-providers --test model_catalog && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Step 2: Run repo verification**

```bash
pnpm verify
```

- [x] **Step 3: Commit and push**

Commit as `feat: add model catalog policy metadata` and push to `origin/main`.
