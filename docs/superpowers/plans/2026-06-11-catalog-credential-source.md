# Catalog Credential Source Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use test-driven-development before implementation. Keep the change additive to catalog metadata.

**Goal:** Make model catalog listings report the safe credential source used to fetch the catalog, matching route resolution audit metadata.

**Architecture:** Extend `ProviderModelCatalog` with `credential_source: Option<String>`. Populate it from the resolved catalog credential source after provider and credential lookup. Do not expose credential values and do not change `list_models()` output.

**Tech Stack:** Rust, `codel00p-providers`, mocked catalog tests, Markdown docs, `pnpm verify`.

---

### Task 1: Add Failing Catalog Credential Test

**Files:**
- Modify: `core/crates/codel00p-providers/tests/model_catalog.rs`

- [x] **Step 1: Write the test**

Add a mocked catalog test that uses `organization_credential(...)`, calls `list_model_catalog(...)`, and asserts `credential_source == Some("organization:<ref>")`.

- [x] **Step 2: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test model_catalog list_model_catalog_reports_credential_source
```

Expected: compile failure because `ProviderModelCatalog` does not expose `credential_source`.

### Task 2: Implement Catalog Credential Source

**Files:**
- Modify: `core/crates/codel00p-providers/src/model_catalog.rs`
- Modify: `core/crates/codel00p-providers/src/client.rs`

- [x] **Step 1: Add the metadata field**

Add `credential_source: Option<String>` to `ProviderModelCatalog`.

- [x] **Step 2: Populate from the resolved credential**

Set the field from the catalog credential source after lookup.

### Task 3: Document The Catalog Source

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Update provider docs**

Mention that richer catalog metadata includes credential source in addition to policy filters.

- [x] **Step 2: Update backlog status**

Mark model catalog listing metadata as including safe credential source.

### Task 4: Verify, Commit, Push

- [x] **Step 1: Run focused checks**

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test model_catalog list_model_catalog_reports_credential_source && cargo test -p codel00p-providers --test model_catalog && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Step 2: Run repo verification**

```bash
pnpm verify
```

- [x] **Step 3: Commit and push**

Commit as `feat: add catalog credential source` and push to `origin/main`.
