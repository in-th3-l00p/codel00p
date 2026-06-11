# Auth Type Policy

**Goal:** Add provider-scoped auth type allowlist policy for route resolution and model catalog listing.

**Architecture:** Extend `ProviderPolicy` with an `allowed_auth_types` map keyed by canonical provider ID, matching the existing credential kind and credential source kind policy style. Enforce the rule after the effective auth type is known and before inference/catalog HTTP calls. Surface allowed auth types in route and catalog policy metadata for audit consumers.

**Tech Stack:** Rust, `codel00p-providers`, provider policy tests, model catalog tests, runtime resolution tests, Markdown docs, `pnpm verify`.

## Test Plan

- [x] **Step 1: Test route auth type policy**

Add a provider policy test that:

- allows an OpenAI route through a configured provider proxy when policy requires `AuthType::CloudProxy`;
- denies a direct OpenAI API-key route when policy requires `AuthType::CloudProxy`;
- verifies provider aliases canonicalize for auth type policy keys.

- [x] **Step 2: Test catalog auth type policy**

Add model catalog tests that:

- deny a custom catalog request when policy requires `AuthType::ApiKey` but the profile reports `AuthType::Custom`;
- return catalog policy metadata with `allowed_auth_types == [AuthType::Custom]` when the catalog request is allowed.

- [x] **Step 3: Test route auth type policy metadata**

Add a runtime resolution test that resolves a cloud-proxy route and verifies route policy metadata reports the configured allowed auth types.

- [ ] **Red Command**

```bash
cd core && cargo test -p codel00p-providers --test provider_policy auth_type_policy && cargo test -p codel00p-providers --test model_catalog auth_type_policy && cargo test -p codel00p-providers --test runtime_resolution auth_type_policy
```

Expected: compile failure because `ProviderPolicy::with_allowed_auth_types` and policy metadata `allowed_auth_types` do not exist yet.

## Implementation Plan

- [x] **Step 1: Add policy storage and builder**

Modify: `core/crates/codel00p-providers/src/policy.rs`

Add `allowed_auth_types: BTreeMap<String, BTreeSet<AuthType>>`, initialize it in constructors, canonicalize provider keys, and add:

```rust
pub fn with_allowed_auth_types<I>(mut self, provider: impl Into<String>, auth_types: I) -> Self
where
    I: IntoIterator<Item = AuthType>,
```

Derive `PartialOrd` and `Ord` for `AuthType` so metadata ordering is deterministic.

- [x] **Step 2: Add policy check**

Add `check_auth_type(provider, auth_type)` that returns `ProviderError::PolicyDenied` with an "auth type is not allowed" reason when the effective route or catalog auth type is outside the allowlist.

- [x] **Step 3: Enforce in route resolution**

Modify: `core/crates/codel00p-providers/src/client.rs`

Compute the effective route auth type once:

- `AuthType::CloudProxy` when `base_url_source == RouteValueSource::CloudProxy`;
- otherwise `profile.auth_type`.

Call `check_auth_type(profile.id, auth_type)` before returning the resolved route and store that same value in `ResolvedInferenceRoute::auth_type`.

- [x] **Step 4: Enforce in model catalog listing**

Call `check_auth_type(profile.id, profile.auth_type)` in `list_model_catalog` before credential checks and HTTP requests.

- [x] **Step 5: Surface audit metadata**

Add `allowed_auth_types: Option<Vec<AuthType>>` to `ProviderRoutePolicy` and `ProviderModelCatalogPolicy`, populate it from `ProviderPolicy::route_policy` and `catalog_policy`, and keep serialization skipped when absent.

## Documentation Plan

- [x] **Step 1: Update provider crate README**

Document that provider policy supports auth type allowlists for requiring CloudProxy, API-key, AWS SDK, GitHub Copilot, custom, or other profile auth modes.

- [x] **Step 2: Update provider status docs**

Mark policy hooks as including provider-scoped auth type rules and safe route/catalog metadata.

- [x] **Step 3: Update backlog**

Update Provider Route Intelligence wording to include auth type rules alongside credential/source-kind and capability rules.

## Verification Plan

- [x] **Focused verification**

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test provider_policy auth_type_policy && cargo test -p codel00p-providers --test model_catalog auth_type_policy && cargo test -p codel00p-providers --test runtime_resolution auth_type_policy && cargo test -p codel00p-providers --test provider_policy && cargo test -p codel00p-providers --test model_catalog && cargo test -p codel00p-providers --test runtime_resolution && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Full repo verification**

```bash
pnpm verify
```

## Commit Plan

- [x] Commit as `feat: add auth type policy` and push to `origin/main`.
