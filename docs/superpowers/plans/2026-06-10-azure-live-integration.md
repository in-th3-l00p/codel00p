# Azure Foundry Live Integration Plan

## Goal

Make Azure AI Foundry deployment routing testable against a real customer
resource with the same opt-in live-test pattern used by the other providers.

Normal test runs remain offline and deterministic. Live Azure calls require
`CODEL00P_INTEGRATION_TESTS=1`, an API key, a resource endpoint, and a
deployment name.

## Task 1: Add Failing Configuration Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/integration_config.rs`
- Modify: `core/crates/codel00p-providers/tests/support/mod.rs`

- [x] **Step 1: Test Azure credential priority**

Assert `CODEL00P_PROVIDER_AZURE_FOUNDRY_API_KEY` wins over
`AZURE_FOUNDRY_API_KEY`.

- [x] **Step 2: Test Azure live config fields**

Assert Azure config reads:

```text
CODEL00P_PROVIDER_AZURE_FOUNDRY_ENDPOINT
CODEL00P_PROVIDER_AZURE_FOUNDRY_DEPLOYMENT
CODEL00P_PROVIDER_AZURE_FOUNDRY_API_VERSION
```

and preserves API key credentials.

- [x] **Step 3: Test defaults and skip messages**

Assert API version defaults to `2024-10-21` and skip messages identify missing
endpoint or deployment configuration without leaking secrets.

## Task 2: Implement Azure Live Config Helper

**Files:**
- Modify: `core/crates/codel00p-providers/tests/support/mod.rs`

- [x] **Step 1: Add `AzureFoundryLiveConfig`**

Expose endpoint, deployment, API version, and credential as a small typed config
used only by live tests.

- [x] **Step 2: Add env var helpers**

Use codel00p-specific variables first, then common Azure aliases:

```text
CODEL00P_PROVIDER_AZURE_FOUNDRY_ENDPOINT
AZURE_FOUNDRY_ENDPOINT
AZURE_OPENAI_ENDPOINT

CODEL00P_PROVIDER_AZURE_FOUNDRY_DEPLOYMENT
AZURE_FOUNDRY_DEPLOYMENT
AZURE_OPENAI_DEPLOYMENT

CODEL00P_PROVIDER_AZURE_FOUNDRY_API_VERSION
AZURE_FOUNDRY_API_VERSION
AZURE_OPENAI_API_VERSION
```

- [x] **Step 3: Add Azure-specific skip and require methods**

Keep live tests ignored by default, but make missing Azure setup actionable.

## Task 3: Add Live Azure Smoke Test

**Files:**
- Create: `core/crates/codel00p-providers/tests/live_azure_foundry.rs`

- [x] **Step 1: Build request from live config**

Use the endpoint as `base_url`, the deployment as both model and deployment,
the configured API version, and a short exact-response prompt.

- [x] **Step 2: Keep the test opt-in**

Mark the test ignored by default and return early with skip messages when live
configuration is incomplete.

## Task 4: Document, Verify, Commit, Push

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/superpowers/plans/2026-06-10-azure-live-integration.md`

- [x] **Step 1: Document live Azure variables**

Add the Azure smoke-test command and the endpoint/deployment/API-version env
variables.

- [x] **Step 2: Verify provider tests**

Run:

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test integration_config && cargo test -p codel00p-providers --test live_azure_foundry -- --ignored && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Step 3: Verify repository**

Run:

```bash
pnpm verify
```

- [x] **Step 4: Commit and push**

Run:

```bash
git add core/crates/codel00p-providers/tests/support/mod.rs core/crates/codel00p-providers/tests/integration_config.rs core/crates/codel00p-providers/tests/live_azure_foundry.rs core/crates/codel00p-providers/README.md docs/providers.md docs/superpowers/plans/2026-06-10-azure-live-integration.md
git commit -m "test: add azure foundry live integration"
git push origin main
```
