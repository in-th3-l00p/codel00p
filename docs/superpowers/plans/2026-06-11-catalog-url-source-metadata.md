# Catalog URL Source Metadata Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add safe model-catalog URL source metadata so catalog audit surfaces can explain where a fetched catalog URL came from.

**Architecture:** Keep catalog URL selection inside `InferenceClient::list_model_catalog`. Add a dedicated `ModelCatalogUrlSource` enum because catalog URLs can come from an explicit `models_url`, a request `base_url` with `/models`, or a provider profile default. Store the source on `ProviderModelCatalog` and export it from the provider crate.

**Tech Stack:** Rust, `codel00p-providers`, mocked model catalog tests, Markdown docs, `pnpm verify`.

---

### Task 1: Add Failing Catalog URL Source Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/model_catalog.rs`

- [x] **Step 1: Test all catalog URL source variants**

Add a mocked catalog test that calls `list_model_catalog(...)` three ways:

- `ModelCatalogRequest::builder("custom").models_url(...)` expects `ModelCatalogUrlSource::RequestModelsUrl`;
- `ModelCatalogRequest::builder("custom").base_url(...)` expects `ModelCatalogUrlSource::RequestBaseUrl`;
- a test registry profile with `models_url` set expects `ModelCatalogUrlSource::ProviderDefault`.

- [x] **Step 2: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test model_catalog catalog_url_source
```

Expected: compile failure because `ModelCatalogUrlSource` and `ProviderModelCatalog::models_url_source` do not exist.

### Task 2: Implement Catalog URL Source Metadata

**Files:**
- Modify: `core/crates/codel00p-providers/src/model_catalog.rs`
- Modify: `core/crates/codel00p-providers/src/client.rs`
- Modify: `core/crates/codel00p-providers/src/lib.rs`

- [x] **Step 1: Add the public enum and catalog field**

Add `ModelCatalogUrlSource` with these variants:

```rust
pub enum ModelCatalogUrlSource {
    RequestModelsUrl,
    RequestBaseUrl,
    ProviderDefault,
}
```

Derive `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `serde::Serialize`, and `serde::Deserialize`. Add `pub models_url_source: ModelCatalogUrlSource` to `ProviderModelCatalog`.

- [x] **Step 2: Track the source during URL selection**

In `InferenceClient::list_model_catalog`, replace the chained `or_else(...)` URL selection with explicit branches returning `(models_url, models_url_source)`.

- [x] **Step 3: Export the enum**

Export `ModelCatalogUrlSource` from `core/crates/codel00p-providers/src/lib.rs`.

### Task 3: Document Catalog URL Source Metadata

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Update provider docs**

Document that catalog metadata includes safe credential source, policy metadata, and catalog URL source metadata.

- [x] **Step 2: Update backlog status**

Mark model catalog listing as having safe catalog URL source metadata.

### Task 4: Verify, Commit, Push

- [x] **Step 1: Run focused checks**

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test model_catalog catalog_url_source && cargo test -p codel00p-providers --test model_catalog && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Step 2: Run repo verification**

```bash
pnpm verify
```

- [x] **Step 3: Commit and push**

Commit as `feat: add catalog url source metadata` and push to `origin/main`.
