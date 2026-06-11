# Credential Source Kind Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add structured credential source kind metadata for route/catalog audit surfaces and introduce a managed-identity credential injection point.

**Architecture:** Keep the existing safe `credential_source` string for human-readable refs, but add a typed `CredentialSourceKind` enum to distinguish configured, environment, organization, cloud-proxy, and managed-identity credentials without parsing source labels. Store the kind with `ResolvedProviderCredential`, attach it to resolved routes and model catalog metadata, and add `InferenceClientBuilder::managed_identity_credential(...)` as the first managed identity integration boundary. Actual cloud token acquisition remains outside this slice.

**Tech Stack:** Rust, `codel00p-providers`, runtime resolution tests, model catalog tests, fallback routing tests, Markdown docs, `pnpm verify`.

---

### Task 1: Add Failing Credential Source Kind Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/runtime_resolution.rs`
- Modify: `core/crates/codel00p-providers/tests/model_catalog.rs`
- Modify: `core/crates/codel00p-providers/tests/fallback_routing.rs`

- [x] **Step 1: Test route credential source kinds**

Extend runtime resolution coverage so it asserts:

```rust
assert_eq!(route.credential_source_kind, Some(CredentialSourceKind::Configured));
assert_eq!(proxy_route.credential_source_kind, Some(CredentialSourceKind::CloudProxy));
assert_eq!(managed_route.credential_source_kind, Some(CredentialSourceKind::ManagedIdentity));
assert_eq!(
    managed_route.credential_source.as_deref(),
    Some("managed_identity:azure/workload-prod")
);
```

- [x] **Step 2: Test catalog credential source kind**

Extend model catalog coverage so organization-managed and managed-identity catalog credentials report:

```rust
assert_eq!(
    result.credential_source_kind,
    Some(CredentialSourceKind::ManagedIdentity)
);
```

- [x] **Step 3: Test response route audit JSON source kind**

Extend `falls_back_when_primary_route_is_rate_limited` to assert:

```rust
assert_eq!(
    route["selected"]["credential_source_kind"],
    json!("Configured")
);
assert_eq!(
    route["attempts"][0]["credential_source_kind"],
    json!("Configured")
);
```

- [x] **Step 4: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test runtime_resolution credential_source_kind && cargo test -p codel00p-providers --test model_catalog credential_source_kind && cargo test -p codel00p-providers --test fallback_routing falls_back_when_primary_route_is_rate_limited
```

Expected: compile failure because `CredentialSourceKind`, `credential_source_kind`, and `managed_identity_credential(...)` do not exist.

### Task 2: Implement Credential Source Kind Metadata

**Files:**
- Modify: `core/crates/codel00p-providers/src/credentials.rs`
- Modify: `core/crates/codel00p-providers/src/registry.rs`
- Modify: `core/crates/codel00p-providers/src/runtime.rs`
- Modify: `core/crates/codel00p-providers/src/model_catalog.rs`
- Modify: `core/crates/codel00p-providers/src/client.rs`
- Modify: `core/crates/codel00p-providers/src/lib.rs`

- [x] **Step 1: Add source kind enum**

Add `CredentialSourceKind` to `credentials.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CredentialSourceKind {
    Configured,
    Environment,
    Organization,
    CloudProxy,
    ManagedIdentity,
}
```

Add `pub source_kind: CredentialSourceKind` to `ResolvedProviderCredential`.

- [x] **Step 2: Populate existing source kinds**

Populate:

- `credential(...)` as `Configured`;
- `organization_credential(...)` as `Organization`;
- `credentials_from_env()`/registry environment credentials as `Environment`;
- provider proxy routes as `CloudProxy`.

- [x] **Step 3: Add managed identity credential injection**

Add:

```rust
pub fn managed_identity_credential(
    mut self,
    provider: impl Into<String>,
    credential: Credential,
    identity_ref: impl Into<String>,
) -> Self
```

It stores source `managed_identity:<identity_ref>` with `CredentialSourceKind::ManagedIdentity`.

- [x] **Step 4: Surface metadata in route and catalog types**

Add `credential_source_kind: Option<CredentialSourceKind>` to `ResolvedInferenceRoute` and `ProviderModelCatalog`, populate both, and serialize route JSON as `"credential_source_kind": route.credential_source_kind`.

- [x] **Step 5: Export source kind**

Update `lib.rs` to export `CredentialSourceKind`.

### Task 3: Document Credential Source Kind Metadata

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Update provider docs**

Document typed credential source kind metadata and the managed identity credential injection boundary.

- [x] **Step 2: Update backlog status**

Mark provider route/catalog audit metadata as including typed credential source kind labels, and mark managed identity work as started with injection/audit metadata rather than live cloud token acquisition.

### Task 4: Verify, Commit, Push

- [x] **Step 1: Run focused checks**

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test runtime_resolution credential_source_kind && cargo test -p codel00p-providers --test model_catalog credential_source_kind && cargo test -p codel00p-providers --test fallback_routing falls_back_when_primary_route_is_rate_limited && cargo test -p codel00p-providers --test runtime_resolution && cargo test -p codel00p-providers --test model_catalog && cargo test -p codel00p-providers --test fallback_routing && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Step 2: Run repo verification**

```bash
pnpm verify
```

- [x] **Step 3: Commit and push**

Commit as `feat: add credential source kind metadata` and push to `origin/main`.
