# Provider Model Catalog Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a provider-neutral model catalog listing API for provider configuration, desktop/cloud setup, and future organization policy.

**Architecture:** Model catalog listing is separate from inference completion. `ModelCatalogRequest` resolves a provider plus optional catalog/base URL overrides, `InferenceClient::list_models` fetches a catalog with the existing provider credential boundary, and `ProviderModel` normalizes IDs plus common display/ownership metadata while preserving provider-specific fields in `provider_data`.

**Tech Stack:** Rust, `codel00p-providers`, `reqwest`, `serde`, `httpmock`, `cargo test`, `cargo clippy`, `pnpm verify`.

---

### Task 1: Add Failing Model Catalog Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/client_interface.rs`
- Create: `core/crates/codel00p-providers/tests/model_catalog.rs`

- [ ] **Step 1: Test request shape**

Add a test that builds:

```rust
ModelCatalogRequest::builder("custom")
    .base_url("http://127.0.0.1:11434/v1")
    .build()
```

Assert provider, base URL override, and absent direct models URL are preserved.

- [ ] **Step 2: Test OpenAI-compatible catalog fetch**

Mock `GET /models` with bearer auth and response:

```json
{
  "object": "list",
  "data": [
    {"id": "gpt-test", "object": "model", "owned_by": "openai"},
    {"id": "claude-via-gateway", "name": "Claude via Gateway", "context_length": 200000}
  ]
}
```

Assert `list_models(...)` returns two normalized `ProviderModel` values,
preserves `owned_by`, maps `name` to `display_name`, and keeps
`context_length` in `provider_data`.

- [ ] **Step 3: Test missing catalog configuration**

Call `list_models` for `custom` without `base_url` or `models_url`. Assert it
returns `ProviderError::MissingBaseUrl { provider: "custom" }`.

- [ ] **Step 4: Test invalid catalog payload**

Mock a successful HTTP response without a `data` array and assert
`ProviderError::InvalidResponse`.

- [ ] **Step 5: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test client_interface model_catalog_request_preserves_overrides
cd core && cargo test -p codel00p-providers --test model_catalog
```

Expected: compile failure because catalog request/types and `list_models` do
not exist.

### Task 2: Add Catalog Types

**Files:**
- Create: `core/crates/codel00p-providers/src/model_catalog.rs`
- Modify: `core/crates/codel00p-providers/src/lib.rs`

- [ ] **Step 1: Add `ModelCatalogRequest`**

Add:

```rust
pub struct ModelCatalogRequest {
    pub provider: String,
    pub base_url: Option<String>,
    pub models_url: Option<String>,
}
```

with a builder supporting `.base_url(...)` and `.models_url(...)`.

- [ ] **Step 2: Add `ProviderModel`**

Add:

```rust
pub struct ProviderModel {
    pub id: String,
    pub display_name: Option<String>,
    pub owned_by: Option<String>,
    pub provider_data: BTreeMap<String, Value>,
}
```

### Task 3: Implement Catalog Fetching

**Files:**
- Modify: `core/crates/codel00p-providers/src/client.rs`
- Modify: `core/crates/codel00p-providers/src/model_catalog.rs`

- [ ] **Step 1: Resolve catalog URL**

Use this priority:

1. `request.models_url`
2. `request.base_url` with `/models` appended
3. provider profile `models_url`

If no URL exists, return `ProviderError::MissingBaseUrl` for that provider.

- [ ] **Step 2: Apply provider policy and credentials**

Resolve aliases through the registry, enforce `ProviderPolicy`, and require an
API-key credential for the canonical provider unless the configured credential
is `Credential::None`.

- [ ] **Step 3: Fetch and parse catalog**

Fetch the URL with `GET`, attach bearer auth for `Credential::ApiKey`, reject
non-2xx HTTP with `ProviderError::Http`, and parse JSON into `Vec<ProviderModel>`.

### Task 4: Document, Verify, Commit, Push

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/agentic-backlog.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/superpowers/plans/2026-06-10-provider-model-catalog.md`

- [ ] **Step 1: Document model catalog support**

Mention `ModelCatalogRequest`, `InferenceClient::list_models`, and the
normalized `ProviderModel` shape.

- [ ] **Step 2: Run targeted provider checks**

Run:

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test client_interface model_catalog_request_preserves_overrides && cargo test -p codel00p-providers --test model_catalog && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [ ] **Step 3: Run repo verification**

Run:

```bash
pnpm verify
```

- [ ] **Step 4: Commit and push**

Run:

```bash
git add core/crates/codel00p-providers/src/model_catalog.rs core/crates/codel00p-providers/src/client.rs core/crates/codel00p-providers/src/lib.rs core/crates/codel00p-providers/tests/client_interface.rs core/crates/codel00p-providers/tests/model_catalog.rs core/crates/codel00p-providers/README.md docs/providers.md docs/agentic-backlog.md docs/product-roadmap.md docs/roadmap.md docs/superpowers/plans/2026-06-10-provider-model-catalog.md
git commit -m "feat: add provider model catalogs"
git push origin main
```
