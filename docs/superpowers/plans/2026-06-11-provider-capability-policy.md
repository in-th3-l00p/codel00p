# Provider Capability Policy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add provider-scoped capability requirements to provider policy so organizations can deny routes or catalogs for providers whose profile capabilities do not satisfy required tools, streaming, vision, or reasoning support.

**Architecture:** Extend `ProviderPolicy` with a `required_provider_capabilities` map keyed by canonical provider ID. Reuse the existing `ProviderCapabilities` and `capabilities_satisfy(...)` helper. Enforce the policy in `InferenceClient::resolve` and `InferenceClient::list_model_catalog` before remote calls, and surface configured provider capability requirements in catalog policy metadata.

**Tech Stack:** Rust, `codel00p-providers`, provider policy tests, model catalog tests, Markdown docs, `pnpm verify`.

---

### Task 1: Add Failing Provider Capability Policy Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/provider_policy.rs`
- Modify: `core/crates/codel00p-providers/tests/model_catalog.rs`

- [x] **Step 1: Test route provider capability requirements**

Add a provider policy test that:

- allows an OpenAI route when policy requires provider tools and reasoning;
- denies a custom provider route when policy requires provider vision;
- verifies provider aliases canonicalize for provider capability policy keys.

- [x] **Step 2: Test catalog provider capability requirements and metadata**

Add model catalog tests that:

- deny a catalog request for custom when policy requires provider vision;
- return catalog policy metadata with `required_provider_capabilities.vision == true` when the provider satisfies the configured requirement.

- [x] **Step 3: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test provider_policy provider_capability_policy && cargo test -p codel00p-providers --test model_catalog provider_capability_policy
```

Expected: compile failure because `ProviderPolicy::with_required_provider_capabilities(...)` and catalog `required_provider_capabilities` metadata do not exist.

### Task 2: Implement Provider Capability Policy

**Files:**
- Modify: `core/crates/codel00p-providers/src/policy.rs`
- Modify: `core/crates/codel00p-providers/src/model_catalog.rs`
- Modify: `core/crates/codel00p-providers/src/client.rs`

- [x] **Step 1: Add policy storage and builder**

Add `required_provider_capabilities: BTreeMap<String, ProviderCapabilities>` to `ProviderPolicy`, initialize it in constructors, canonicalize provider keys, and add:

```rust
pub fn with_required_provider_capabilities(
    mut self,
    provider: impl Into<String>,
    capabilities: ProviderCapabilities,
) -> Self
```

- [x] **Step 2: Add policy check**

Add `check_provider_capabilities(provider, capabilities)` returning `ProviderError::PolicyDenied` when a provider has configured capability requirements and the resolved profile capabilities do not satisfy them.

- [x] **Step 3: Enforce route and catalog checks**

Call `check_provider_capabilities(...)` in `InferenceClient::resolve` and `InferenceClient::list_model_catalog` before remote calls.

- [x] **Step 4: Surface catalog policy metadata**

Add `required_provider_capabilities: ProviderCapabilities` to `ProviderModelCatalogPolicy`.

### Task 3: Document Provider Capability Policy

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Update provider docs**

Document that provider policy supports provider capability requirements and catalog policy metadata reports them.

- [x] **Step 2: Update backlog status**

Mark policy hooks as including provider capability requirements.

### Task 4: Verify, Commit, Push

- [x] **Step 1: Run focused checks**

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test provider_policy provider_capability_policy && cargo test -p codel00p-providers --test model_catalog provider_capability_policy && cargo test -p codel00p-providers --test provider_policy && cargo test -p codel00p-providers --test model_catalog && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Step 2: Run repo verification**

```bash
pnpm verify
```

- [x] **Step 3: Commit and push**

Commit as `feat: add provider capability policy` and push to `origin/main`.
