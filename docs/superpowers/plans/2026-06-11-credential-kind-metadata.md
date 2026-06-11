# Credential Kind Metadata Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add safe credential kind metadata to route and catalog audit surfaces without exposing credential values.

**Architecture:** Keep credential secrets inside `Credential`. Add a small public `CredentialKind` enum plus `Credential::kind()` so audit structs can record whether a resolved credential is API-key, AWS SigV4, or intentionally unauthenticated. Attach `credential_kind` to `ResolvedInferenceRoute` and `ProviderModelCatalog`, and include it in response route metadata.

**Tech Stack:** Rust, `codel00p-providers`, route and catalog tests, Markdown docs, `pnpm verify`.

---

### Task 1: Add Failing Credential Kind Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/runtime_resolution.rs`
- Modify: `core/crates/codel00p-providers/tests/model_catalog.rs`

- [x] **Step 1: Test route credential kind metadata**

Update route resolution tests to assert:

- configured API-key routes report `Some(CredentialKind::ApiKey)`;
- cloud proxy routes report `Some(CredentialKind::ApiKey)`;
- AWS SigV4 environment-loaded Bedrock routes report `Some(CredentialKind::AwsSigV4)`.

- [x] **Step 2: Test catalog credential kind metadata**

Update model catalog tests to assert:

- organization API-key catalog credentials report `Some(CredentialKind::ApiKey)`;
- intentionally unauthenticated catalog credentials report `Some(CredentialKind::None)`.

- [x] **Step 3: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test runtime_resolution credential_kind && cargo test -p codel00p-providers --test model_catalog credential_kind
```

Expected: compile failure because `CredentialKind`, `credential_kind` route metadata, and `credential_kind` catalog metadata do not exist.

### Task 2: Implement Credential Kind Metadata

**Files:**
- Modify: `core/crates/codel00p-providers/src/credentials.rs`
- Modify: `core/crates/codel00p-providers/src/runtime.rs`
- Modify: `core/crates/codel00p-providers/src/model_catalog.rs`
- Modify: `core/crates/codel00p-providers/src/client.rs`
- Modify: `core/crates/codel00p-providers/src/lib.rs`

- [x] **Step 1: Add `CredentialKind` and `Credential::kind()`**

Add:

```rust
pub enum CredentialKind {
    ApiKey,
    AwsSigV4,
    None,
}
```

Derive `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `serde::Serialize`, and `serde::Deserialize`. Implement `Credential::kind()` by matching on the existing credential variants.

- [x] **Step 2: Attach kind to route and catalog structs**

Add `pub credential_kind: Option<CredentialKind>` to both `ResolvedInferenceRoute` and `ProviderModelCatalog`.

- [x] **Step 3: Populate route and catalog metadata**

In `InferenceClient::resolve`, set `credential_kind` from the cloud proxy credential or resolved provider credential. In `InferenceClient::list_model_catalog`, set `credential_kind` from the resolved catalog credential.

- [x] **Step 4: Include kind in response route metadata**

Add `credential_kind` to the JSON emitted by `route_metadata(...)`.

- [x] **Step 5: Export `CredentialKind`**

Export `CredentialKind` from `core/crates/codel00p-providers/src/lib.rs`.

### Task 3: Document Credential Kind Metadata

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Update provider docs**

Document that route and catalog audit metadata record safe credential source and kind metadata, never credential values.

- [x] **Step 2: Update backlog status**

Mark route and catalog audit metadata as carrying safe credential kind labels.

### Task 4: Verify, Commit, Push

- [x] **Step 1: Run focused checks**

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test runtime_resolution credential_kind && cargo test -p codel00p-providers --test model_catalog credential_kind && cargo test -p codel00p-providers --test runtime_resolution && cargo test -p codel00p-providers --test model_catalog && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Step 2: Run repo verification**

```bash
pnpm verify
```

- [x] **Step 3: Commit and push**

Commit as `feat: add credential kind metadata` and push to `origin/main`.
