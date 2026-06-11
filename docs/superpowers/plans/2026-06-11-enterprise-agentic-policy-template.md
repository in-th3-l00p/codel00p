# Enterprise Agentic Policy Template Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a conservative provider policy template that allows direct enterprise providers and surfaces agentic model capability requirements in catalog listings.

**Architecture:** Keep `ProviderPolicy` as the single policy object. Add `ProviderPolicy::enterprise_direct_agentic()` as an additive constructor that starts from `enterprise_direct()` and applies `ProviderCapabilities::agentic()` as required catalog model capabilities for each direct enterprise provider. Route-time provider/model checks remain unchanged; capability requirements remain catalog-only where `ProviderModel.capability_flags` exist.

**Tech Stack:** Rust, `codel00p-providers`, mocked model catalog tests, provider policy tests, Markdown docs, `pnpm verify`.

---

### Task 1: Add Failing Template Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/provider_policy.rs`
- Modify: `core/crates/codel00p-providers/tests/model_catalog.rs`

- [x] **Step 1: Test route-level provider behavior**

Add a provider policy test that builds a client with `ProviderPolicy::enterprise_direct_agentic()` and verifies:

- direct provider `openai` resolves;
- broker provider `openrouter` is denied.

- [x] **Step 2: Test catalog capability metadata and filtering**

Add a mocked `github-models` catalog test that returns:

- one model with `tool-calling`, `streaming`, and `reasoning` labels;
- one model with only `tool-calling`;
- one model with only `streaming`.

Build a client with `ProviderPolicy::enterprise_direct_agentic()` and assert:

- `list_model_catalog(...)` returns only the fully agentic model;
- catalog policy metadata reports required tools, streaming, and reasoning;
- required vision remains false.

- [x] **Step 3: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test provider_policy enterprise_direct_agentic && cargo test -p codel00p-providers --test model_catalog list_model_catalog_applies_enterprise_agentic_template
```

Expected: compile failure because `ProviderPolicy::enterprise_direct_agentic()` does not exist.

### Task 2: Implement Template Constructor

**Files:**
- Modify: `core/crates/codel00p-providers/src/policy.rs`

- [x] **Step 1: Add constructor**

Implement `ProviderPolicy::enterprise_direct_agentic()` by starting from `enterprise_direct()` and inserting `ProviderCapabilities::agentic()` into `required_model_capabilities` for every provider in `ENTERPRISE_DIRECT_PROVIDERS`.

- [x] **Step 2: Keep alias canonicalization unchanged**

Rely on the existing builder `canonicalize(...)` path, so direct provider IDs and future aliases continue to behave consistently.

### Task 3: Document Template

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Update provider docs**

Document that `enterprise_direct_agentic()` combines the direct-provider allowlist with catalog filtering for tool, streaming, and reasoning model capability flags.

- [x] **Step 2: Update roadmap/backlog status**

Mark provider catalog metadata as surfaced in policy templates.

### Task 4: Verify, Commit, Push

- [x] **Step 1: Run focused checks**

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test provider_policy enterprise_direct_agentic && cargo test -p codel00p-providers --test model_catalog list_model_catalog_applies_enterprise_agentic_template && cargo test -p codel00p-providers --test provider_policy && cargo test -p codel00p-providers --test model_catalog && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Step 2: Run repo verification**

```bash
pnpm verify
```

- [x] **Step 3: Commit and push**

Commit as `feat: add enterprise agentic policy template` and push to `origin/main`.
