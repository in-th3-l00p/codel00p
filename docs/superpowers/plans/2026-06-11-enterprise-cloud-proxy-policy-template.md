# Enterprise Cloud Proxy Policy Template

**Goal:** Add a conservative built-in provider policy template for organizations that require enterprise direct-provider routes to resolve through codel00p-managed cloud proxies.

**Architecture:** Keep `ProviderPolicy` as the single policy object. Add `ProviderPolicy::enterprise_cloud_proxy()` as an additive constructor that starts from `enterprise_direct()` and applies `AuthType::CloudProxy` as the only allowed auth type for every direct enterprise provider. The template should preserve the existing broker/custom exclusion boundary and surface the configured auth type allowlist in route policy metadata.

**Tech Stack:** Rust, `codel00p-providers`, provider policy tests, Markdown docs, `pnpm verify`.

## Test Plan

- [x] **Step 1: Test proxied direct provider routes**

Add a provider policy test that builds a client with an OpenAI provider proxy and `ProviderPolicy::enterprise_cloud_proxy()`, then verifies:

- route resolution succeeds for `openai`;
- `route.auth_type == AuthType::CloudProxy`;
- `route.base_url_source == RouteValueSource::CloudProxy`;
- route policy metadata reports `allowed_auth_types == [AuthType::CloudProxy]`.

- [x] **Step 2: Test direct API-key routes are denied**

With the same template but no provider proxy, resolve `openai` directly and assert `ProviderError::PolicyDenied` with an auth type denial reason.

- [x] **Step 3: Test broker/custom endpoints remain explicit opt-ins**

With the same template and an OpenRouter provider proxy configured, resolve `openrouter` and assert policy denial because brokers are outside the enterprise direct-provider boundary.

- [ ] **Red Command**

```bash
cd core && cargo test -p codel00p-providers --test provider_policy enterprise_cloud_proxy
```

Expected: compile failure because `ProviderPolicy::enterprise_cloud_proxy()` does not exist yet.

## Implementation Plan

- [x] **Step 1: Add template constructor**

Modify: `core/crates/codel00p-providers/src/policy.rs`

Implement `ProviderPolicy::enterprise_cloud_proxy()` by starting from `enterprise_direct()` and inserting `[AuthType::CloudProxy]` into `allowed_auth_types` for every provider in `ENTERPRISE_DIRECT_PROVIDERS`.

- [x] **Step 2: Preserve existing policy behavior**

No new enforcement path is required. Reuse the existing provider allowlist and auth type policy checks so the template denies direct API-key routes and broker/custom endpoints before inference.

## Documentation Plan

- [x] **Step 1: Update provider crate README**

Document that `enterprise_cloud_proxy()` combines direct-provider allowlists with a CloudProxy auth-type requirement.

- [x] **Step 2: Update provider status docs**

Mark the cloud-proxy template as an implemented enterprise policy template.

- [x] **Step 3: Update backlog**

Update Provider Route Intelligence wording to include enterprise cloud-proxy policy templates.

## Verification Plan

- [x] **Focused verification**

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test provider_policy enterprise_cloud_proxy && cargo test -p codel00p-providers --test provider_policy && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Full repo verification**

```bash
pnpm verify
```

## Commit Plan

- [x] Commit as `feat: add enterprise cloud proxy policy template` and push to `origin/main`.
