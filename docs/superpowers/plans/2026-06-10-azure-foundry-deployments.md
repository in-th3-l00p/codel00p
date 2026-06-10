# Azure Foundry Deployment Routing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Azure AI Foundry deployment-aware chat completions routing to `codel00p-providers`.

**Architecture:** Azure should not be treated as a plain OpenAI-compatible base URL. The provider profile will use a dedicated Azure chat completions API mode and transport that builds `/openai/deployments/{deployment}/chat/completions?api-version={version}` URLs, sends `api-key`, and normalizes responses through the existing Chat Completions response parser. Request deployment/API version settings stay explicit on `InferenceRequest` so harness, CLI, desktop, and cloud can configure them without string-concatenating URLs.

**Tech Stack:** Rust, `codel00p-providers`, `httpmock`, Microsoft Foundry Azure OpenAI REST shape, `cargo test`, `cargo clippy`, `pnpm verify`.

---

### Task 1: Add Failing Azure Request And Transport Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/client_interface.rs`
- Create: `core/crates/codel00p-providers/tests/azure_foundry_transport.rs`

- [x] **Step 1: Test request deployment options**

Add a test building:

```rust
InferenceRequest::builder("azure", "gpt-4.1")
    .deployment("team-chat")
    .api_version("2024-10-21")
    .message(ChatMessage::user("hello"))
    .build()
```

Assert provider, model, deployment, and API version are preserved.

- [x] **Step 2: Test Azure deployment URL and API key header**

Mock Azure chat completions:

```text
POST /openai/deployments/team-chat/chat/completions?api-version=2024-10-21
api-key: azure-key
```

Assert the JSON body contains `messages`, `temperature`, and `max_tokens`, but
does not require an OpenAI-style `/chat/completions` URL or bearer token.

- [x] **Step 3: Test default deployment and API version**

Build a request without deployment or API version. Assert the transport uses the
request model as the deployment and defaults API version to `2024-10-21`.

- [x] **Step 4: Test missing Azure credential**

Build an Azure request with a base URL and no credential. Assert
`ProviderError::MissingCredential { provider: "azure-foundry" }`.

- [x] **Step 5: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test client_interface request_builder_preserves_deployment_options
cd core && cargo test -p codel00p-providers --test azure_foundry_transport
```

Expected: compile failure because deployment/API version request options and
the Azure-specific transport do not exist.

### Task 2: Add Request Options And API Mode

**Files:**
- Modify: `core/crates/codel00p-providers/src/request.rs`
- Modify: `core/crates/codel00p-providers/src/profile.rs`
- Modify: `core/crates/codel00p-providers/src/registry.rs`
- Modify: `core/crates/codel00p-providers/src/client.rs`

- [x] **Step 1: Extend `InferenceRequest`**

Add:

```rust
pub deployment: Option<String>,
pub api_version: Option<String>,
```

with builder methods:

```rust
pub fn deployment(mut self, value: impl Into<String>) -> Self
pub fn api_version(mut self, value: impl Into<String>) -> Self
```

- [x] **Step 2: Add Azure API mode**

Add `ApiMode::AzureChatCompletions` and set the Azure profile to that mode.

- [x] **Step 3: Use Azure max token parameter**

Set the Azure profile output token parameter to `OutputTokenParameter::MaxTokens`
for the deployment chat completions REST shape.

### Task 3: Implement Azure Chat Completions Transport

**Files:**
- Create: `core/crates/codel00p-providers/src/transports/azure_chat_completions.rs`
- Modify: `core/crates/codel00p-providers/src/transports/mod.rs`
- Modify: `core/crates/codel00p-providers/src/transports/chat_completions.rs`
- Modify: `core/crates/codel00p-providers/src/client.rs`

- [x] **Step 1: Reuse Chat Completions wire helpers**

Expose the existing Chat Completions request/response wire helpers as
`pub(crate)` so Azure can reuse parsing and message/tool serialization.

- [x] **Step 2: Implement URL construction**

Build:

```text
{base_url}/openai/deployments/{urlencoded_deployment}/chat/completions?api-version={urlencoded_api_version}
```

Use request `deployment` when present; otherwise use request `model`. Default
API version is `2024-10-21`.

- [x] **Step 3: Implement Azure HTTP behavior**

Send `api-key` with `Credential::ApiKey`, reject non-API-key credentials, post
the JSON body, reject non-2xx status codes, and normalize the response.

- [x] **Step 4: Wire client dispatch**

Add an `ApiMode::AzureChatCompletions` match arm in `InferenceClient`.

### Task 4: Document, Verify, Commit, Push

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/superpowers/plans/2026-06-10-azure-foundry-deployments.md`

- [x] **Step 1: Document Azure deployment routing**

Mention deployment/API version request options and Azure-specific chat
completions URL behavior.

- [x] **Step 2: Run targeted provider checks**

Run:

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test client_interface request_builder_preserves_deployment_options && cargo test -p codel00p-providers --test azure_foundry_transport && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Step 3: Run repo verification**

Run:

```bash
pnpm verify
```

- [x] **Step 4: Commit and push**

Run:

```bash
git add core/crates/codel00p-providers/src/request.rs core/crates/codel00p-providers/src/profile.rs core/crates/codel00p-providers/src/registry.rs core/crates/codel00p-providers/src/client.rs core/crates/codel00p-providers/src/transports/mod.rs core/crates/codel00p-providers/src/transports/chat_completions.rs core/crates/codel00p-providers/src/transports/azure_chat_completions.rs core/crates/codel00p-providers/tests/client_interface.rs core/crates/codel00p-providers/tests/azure_foundry_transport.rs core/crates/codel00p-providers/README.md docs/providers.md docs/roadmap.md docs/product-roadmap.md docs/superpowers/plans/2026-06-10-azure-foundry-deployments.md
git commit -m "feat: add azure foundry deployment routing"
git push origin main
```
