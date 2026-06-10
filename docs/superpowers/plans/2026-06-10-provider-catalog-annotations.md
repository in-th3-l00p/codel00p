# Provider Catalog Annotations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Normalize provider-specific catalog annotations so setup and UI surfaces can read known provider hints without scraping raw provider JSON.

**Architecture:** Add an additive `ProviderModelAnnotations` struct on `ProviderModel`. Parse GitHub Models-style catalog fields (`registry`, `html_url`, `version`, `rate_limit_tier`, and `tags`) into typed annotation fields while preserving the raw consumed keys in `provider_data`.

**Tech Stack:** Rust, serde, `codel00p-providers`, mocked model catalog tests.

---

### Task 1: Add Red Annotation Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/model_catalog.rs`

- [ ] **Step 1: Expand GitHub catalog fixture**

Add `registry`, `html_url`, `version`, and `tags` to the mocked GitHub Models
catalog entry. Keep `rate_limit_tier` in the fixture.

- [ ] **Step 2: Assert typed annotations**

Assert the returned `ProviderModel` exposes:

```rust
assert_eq!(models[0].annotations.registry.as_deref(), Some("github"));
assert_eq!(
    models[0].annotations.html_url.as_deref(),
    Some("https://github.com/marketplace/models/openai/gpt-4.1-mini")
);
assert_eq!(models[0].annotations.version.as_deref(), Some("2025-04-14"));
assert_eq!(models[0].annotations.rate_limit_tier.as_deref(), Some("low"));
assert_eq!(models[0].annotations.tags, vec!["reasoning", "multimodal"]);
```

- [ ] **Step 3: Assert raw provider preservation**

Assert the same consumed annotation keys remain available in
`models[0].provider_data`.

- [ ] **Step 4: Run tests to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-providers --test model_catalog list_models_normalizes_github_models_catalog
```

Expected: FAIL because `ProviderModel.annotations` does not exist.

### Task 2: Implement Typed Annotations

**Files:**
- Modify: `core/crates/codel00p-providers/src/model_catalog.rs`
- Modify: `core/crates/codel00p-providers/src/lib.rs`

- [ ] **Step 1: Add annotation struct**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ProviderModelAnnotations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_tier: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}
```

- [ ] **Step 2: Add empty check**

Implement `ProviderModelAnnotations::is_empty` so `ProviderModel.annotations`
can skip serialization when no annotation values are present.

- [ ] **Step 3: Parse annotation wire fields**

Add optional `registry`, `html_url`, `version`, `rate_limit_tier`, and default
`tags` fields to `ModelCatalogWireModel`.

- [ ] **Step 4: Preserve consumed raw fields**

Reinsert present annotation fields into `provider_data` before returning the
normalized model.

- [ ] **Step 5: Populate normalized annotations**

Set `ProviderModel.annotations` from the parsed annotation wire fields.

- [ ] **Step 6: Export annotation type**

Export `ProviderModelAnnotations` from `core/crates/codel00p-providers/src/lib.rs`.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/product-roadmap.md`

- [ ] **Step 1: Document annotation metadata**

Mention typed provider catalog annotations alongside descriptions,
capabilities, modalities, and limits.

- [ ] **Step 2: Run provider verification**

Run:

```bash
cd core
cargo fmt --all -- --check
cargo test -p codel00p-providers --test model_catalog
cargo test -p codel00p-providers
cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

Expected: all commands pass.

- [ ] **Step 3: Run repo verification**

Run:

```bash
pnpm verify
```

Expected: PASS.

- [ ] **Step 4: Commit and push**

Run:

```bash
git add docs/superpowers/plans/2026-06-10-provider-catalog-annotations.md core/crates/codel00p-providers docs
git commit -m "feat: add provider catalog annotations"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
