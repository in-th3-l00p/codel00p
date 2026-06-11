# Credential Kind Policy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add provider-scoped credential kind allowlist policy for route resolution and model catalog listing.

**Architecture:** Extend `ProviderPolicy` with an `allowed_credential_kinds` map keyed by canonical provider ID, matching the existing model and capability policy style. Enforce the policy after credential kind metadata is resolved and before inference/catalog HTTP calls. Surface allowed credential kinds in catalog policy metadata for audit consumers.

**Tech Stack:** Rust, `codel00p-providers`, provider policy tests, model catalog tests, Markdown docs, `pnpm verify`.

---

### Task 1: Add Failing Credential Kind Policy Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/provider_policy.rs`
- Modify: `core/crates/codel00p-providers/tests/model_catalog.rs`

- [x] **Step 1: Test route credential kind allowlist**

Add a provider policy test that:

- allows an OpenAI API-key route when policy requires `CredentialKind::ApiKey`;
- denies a custom `Credential::None` route when policy requires `CredentialKind::ApiKey`;
- verifies provider aliases canonicalize for credential kind policy keys.

- [x] **Step 2: Test catalog credential kind allowlist and metadata**

Add model catalog tests that:

- deny a catalog request using `Credential::None` when policy requires `CredentialKind::ApiKey`;
- return catalog policy metadata with `allowed_credential_kinds == [CredentialKind::ApiKey]` when the catalog request uses an API-key credential.

- [x] **Step 3: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test provider_policy credential_kind_policy && cargo test -p codel00p-providers --test model_catalog credential_kind_policy
```

Expected: compile failure because `ProviderPolicy::with_allowed_credential_kinds(...)` and catalog `allowed_credential_kinds` metadata do not exist.

### Task 2: Implement Credential Kind Policy

**Files:**
- Modify: `core/crates/codel00p-providers/src/credentials.rs`
- Modify: `core/crates/codel00p-providers/src/policy.rs`
- Modify: `core/crates/codel00p-providers/src/model_catalog.rs`
- Modify: `core/crates/codel00p-providers/src/client.rs`

- [x] **Step 1: Make `CredentialKind` sortable**

Derive `PartialOrd` and `Ord` for `CredentialKind` so policy metadata is deterministic.

- [x] **Step 2: Add policy storage and builder**

Add `allowed_credential_kinds: BTreeMap<String, BTreeSet<CredentialKind>>` to `ProviderPolicy`, initialize it in constructors, canonicalize provider keys, and add:

```rust
pub fn with_allowed_credential_kinds<I>(
    mut self,
    provider: impl Into<String>,
    kinds: I,
) -> Self
where
    I: IntoIterator<Item = CredentialKind>,
```

- [x] **Step 3: Add policy check**

Add `check_credential_kind(provider, credential_kind)` returning `ProviderError::PolicyDenied` when a provider has a configured credential kind allowlist and the resolved kind is missing or not allowed.

- [x] **Step 4: Enforce route and catalog checks**

Call `check_credential_kind(...)` in `InferenceClient::resolve` and `InferenceClient::list_model_catalog` before remote calls.

- [x] **Step 5: Surface catalog policy metadata**

Add `allowed_credential_kinds: Option<Vec<CredentialKind>>` to `ProviderModelCatalogPolicy`.

### Task 3: Document Credential Kind Policy

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Update provider docs**

Document that provider policy supports credential kind allowlists and catalog policy metadata reports them.

- [x] **Step 2: Update backlog status**

Mark policy hooks as including provider-scoped credential kind rules.

### Task 4: Verify, Commit, Push

- [x] **Step 1: Run focused checks**

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test provider_policy credential_kind_policy && cargo test -p codel00p-providers --test model_catalog credential_kind_policy && cargo test -p codel00p-providers --test provider_policy && cargo test -p codel00p-providers --test model_catalog && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Step 2: Run repo verification**

```bash
pnpm verify
```

- [x] **Step 3: Commit and push**

Commit as `feat: add credential kind policy` and push to `origin/main`.
