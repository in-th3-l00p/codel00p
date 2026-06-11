# Enterprise Managed Identity Policy Template

**Goal:** Add a conservative built-in provider policy template for organizations that require enterprise direct-provider routes to use codel00p-managed identity credential injection.

**Architecture:** Keep `ProviderPolicy` as the single policy object. Add `ProviderPolicy::enterprise_managed_identity()` as an additive constructor that starts from `enterprise_direct()` and applies `CredentialSourceKind::ManagedIdentity` as the only allowed credential source kind for every direct enterprise provider. Reuse the existing credential source kind enforcement and route policy metadata.

**Tech Stack:** Rust, `codel00p-providers`, provider policy tests, Markdown docs, `pnpm verify`.

## Test Plan

- [x] **Step 1: Test managed identity direct provider routes**

Add a provider policy test that builds a client with a managed identity OpenAI credential and `ProviderPolicy::enterprise_managed_identity()`, then verifies:

- route resolution succeeds for `openai`;
- `credential_source_kind == Some(CredentialSourceKind::ManagedIdentity)`;
- route policy metadata reports `allowed_credential_source_kinds == [CredentialSourceKind::ManagedIdentity]`.

- [x] **Step 2: Test configured credentials are denied**

With the same template but a configured OpenAI credential, resolve `openai` and assert `ProviderError::PolicyDenied` with a credential source kind denial reason.

- [x] **Step 3: Test broker/custom endpoints remain explicit opt-ins**

With the same template and a managed identity OpenRouter credential, resolve `openrouter` and assert policy denial because brokers are outside the enterprise direct-provider boundary.

- [ ] **Red Command**

```bash
cd core && cargo test -p codel00p-providers --test provider_policy enterprise_managed_identity
```

Expected: compile failure because `ProviderPolicy::enterprise_managed_identity()` does not exist yet.

## Implementation Plan

- [x] **Step 1: Add template constructor**

Modify: `core/crates/codel00p-providers/src/policy.rs`

Implement `ProviderPolicy::enterprise_managed_identity()` by starting from `enterprise_direct()` and inserting `[CredentialSourceKind::ManagedIdentity]` into `allowed_credential_source_kinds` for every provider in `ENTERPRISE_DIRECT_PROVIDERS`.

- [x] **Step 2: Preserve existing policy behavior**

No new enforcement path is required. Reuse the existing provider allowlist and credential source kind policy checks so the template denies configured/environment/cloud-proxy credentials and broker/custom endpoints before inference.

## Documentation Plan

- [x] **Step 1: Update provider crate README**

Document that `enterprise_managed_identity()` combines direct-provider allowlists with a managed identity credential-source requirement.

- [x] **Step 2: Update provider status docs**

Mark the managed-identity template as an implemented enterprise policy template.

- [x] **Step 3: Update backlog**

Update Provider Route Intelligence wording to include managed identity policy templates.

## Verification Plan

- [x] **Focused verification**

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test provider_policy enterprise_managed_identity && cargo test -p codel00p-providers --test provider_policy && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Full repo verification**

```bash
pnpm verify
```

## Commit Plan

- [x] Commit as `feat: add enterprise managed identity policy template` and push to `origin/main`.
