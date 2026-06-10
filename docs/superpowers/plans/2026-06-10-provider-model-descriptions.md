# Provider Model Descriptions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Normalize model description text from provider catalogs so UI and policy surfaces do not need to dig through provider-specific JSON.

**Architecture:** Extend `ProviderModel` with an additive `description` field. Map OpenAI-compatible `description` and GitHub Models `summary` into that field while preserving the raw consumed values in `provider_data`.

**Tech Stack:** Rust, serde, `codel00p-providers`, mocked model catalog tests.

---

### Task 1: Add Red Catalog Description Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/model_catalog.rs`

- [ ] **Step 1: Test OpenAI-compatible description**

Add `description` to a mocked OpenAI-compatible catalog model and assert
`ProviderModel.description` is populated.

- [ ] **Step 2: Test GitHub summary mapping**

Assert the existing GitHub Models `summary` fixture maps to
`ProviderModel.description` while remaining present in `provider_data`.

- [ ] **Step 3: Run tests to verify failure**

Run: `cargo test -p codel00p-providers --test model_catalog`

Expected: FAIL because `ProviderModel.description` does not exist.

### Task 2: Implement Description Normalization

**Files:**
- Modify: `core/crates/codel00p-providers/src/model_catalog.rs`

- [ ] **Step 1: Extend `ProviderModel`**

Add `pub description: Option<String>`.

- [ ] **Step 2: Parse provider fields**

Read optional `description` and `summary` from `ModelCatalogWireModel`.

- [ ] **Step 3: Preserve raw provider data**

Reinsert consumed `description` and `summary` fields into `provider_data` when
present.

- [ ] **Step 4: Prefer explicit description**

Set normalized `description` to `description.or(summary)`.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/product-roadmap.md`

- [ ] **Step 1: Document description metadata**

Mention model descriptions as typed catalog metadata alongside capabilities,
modalities, and limits.

- [ ] **Step 2: Run provider verification**

Run:

```bash
cargo fmt --all
cargo test -p codel00p-providers --test model_catalog
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
git add docs/superpowers/plans/2026-06-10-provider-model-descriptions.md core/crates/codel00p-providers docs
git commit -m "feat: normalize provider model descriptions"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
