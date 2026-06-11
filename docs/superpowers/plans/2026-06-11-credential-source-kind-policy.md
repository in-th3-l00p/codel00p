# Credential Source Kind Policy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add provider-scoped credential source kind allowlist policy for route resolution and model catalog listing.

**Architecture:** Extend `ProviderPolicy` with an `allowed_credential_source_kinds` map keyed by canonical provider ID. Enforce it after credential source kind metadata is resolved and before inference/catalog HTTP calls. Surface configured source-kind allowlists in route and catalog policy metadata so teams can require managed identity, organization-managed, environment, configured, or cloud-proxy credential sources without parsing source labels.

**Tech Stack:** Rust, `codel00p-providers`, provider policy tests, model catalog tests, fallback route metadata tests, Markdown docs, `pnpm verify`.

---

### Task 1: Add Failing Credential Source Kind Policy Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/provider_policy.rs`
- Modify: `core/crates/codel00p-providers/tests/model_catalog.rs`
- Modify: `core/crates/codel00p-providers/tests/runtime_resolution.rs`

- [x] **Step 1: Test route source kind allowlist**

Add a provider policy test that:

- allows an OpenAI route when policy requires `CredentialSourceKind::ManagedIdentity` and the credential was injected through `managed_identity_credential(...)`;
- denies the same provider when policy requires managed identity but the credential was injected through `credential(...)`;
- verifies provider aliases canonicalize for source-kind policy keys.

- [x] **Step 2: Test catalog source kind allowlist and metadata**

Add model catalog tests that:

- deny a catalog request using configured credentials when policy requires managed identity;
- return catalog policy metadata with `allowed_credential_source_kinds == [CredentialSourceKind::ManagedIdentity]` for a managed-identity catalog request.

- [x] **Step 3: Test route policy metadata includes source kind allowlist**

Extend route resolution policy metadata coverage to assert:

```rust
assert_eq!(
    route.policy.allowed_credential_source_kinds,
    Some(vec![CredentialSourceKind::ManagedIdentity])
);
```

- [x] **Step 4: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test provider_policy credential_source_kind_policy && cargo test -p codel00p-providers --test model_catalog credential_source_kind_policy && cargo test -p codel00p-providers --test runtime_resolution credential_source_kind_policy
```

Expected: compile failure because `ProviderPolicy::with_allowed_credential_source_kinds(...)` and policy metadata fields do not exist.

### Task 2: Implement Credential Source Kind Policy

**Files:**
- Modify: `core/crates/codel00p-providers/src/credentials.rs`
- Modify: `core/crates/codel00p-providers/src/policy.rs`
- Modify: `core/crates/codel00p-providers/src/runtime.rs`
- Modify: `core/crates/codel00p-providers/src/model_catalog.rs`
- Modify: `core/crates/codel00p-providers/src/client.rs`

- [x] **Step 1: Make `CredentialSourceKind` sortable**

Derive `PartialOrd` and `Ord` for `CredentialSourceKind` so policy metadata is deterministic.

- [x] **Step 2: Add policy storage and builder**

Add `allowed_credential_source_kinds: BTreeMap<String, BTreeSet<CredentialSourceKind>>` to `ProviderPolicy`, initialize it in constructors, canonicalize provider keys, and add:

```rust
pub fn with_allowed_credential_source_kinds<I>(
    mut self,
    provider: impl Into<String>,
    kinds: I,
) -> Self
where
    I: IntoIterator<Item = CredentialSourceKind>,
```

- [x] **Step 3: Add policy check**

Add `check_credential_source_kind(provider, credential_source_kind)` returning `ProviderError::PolicyDenied` when a provider has a configured source kind allowlist and the resolved kind is missing or not allowed.

- [x] **Step 4: Enforce route and catalog checks**

Call `check_credential_source_kind(...)` in `InferenceClient::resolve` and `InferenceClient::list_model_catalog` before remote calls.

- [x] **Step 5: Surface route and catalog policy metadata**

Add `allowed_credential_source_kinds: Option<Vec<CredentialSourceKind>>` to `ProviderRoutePolicy` and `ProviderModelCatalogPolicy`.

### Task 3: Document Credential Source Kind Policy

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Update provider docs**

Document that provider policy supports credential source kind allowlists and route/catalog policy metadata reports them.

- [x] **Step 2: Update backlog status**

Mark provider policy hooks as including provider-scoped credential source kind rules.

### Task 4: Verify, Commit, Push

- [x] **Step 1: Run focused checks**

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test provider_policy credential_source_kind_policy && cargo test -p codel00p-providers --test model_catalog credential_source_kind_policy && cargo test -p codel00p-providers --test runtime_resolution credential_source_kind_policy && cargo test -p codel00p-providers --test provider_policy && cargo test -p codel00p-providers --test model_catalog && cargo test -p codel00p-providers --test runtime_resolution && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Step 2: Run repo verification**

```bash
pnpm verify
```

- [x] **Step 3: Commit and push**

Commit as `feat: add credential source kind policy` and push to `origin/main`.
