# Route Policy Metadata Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add safe provider policy metadata to resolved inference routes and response route audit JSON.

**Architecture:** Introduce a route-specific `ProviderRoutePolicy` snapshot that mirrors the route-relevant policy filters without renaming the existing catalog policy type. Populate it from canonicalized `ProviderPolicy` during route resolution, attach it to `ResolvedInferenceRoute`, and serialize it into `codel00p_route.selected` and each resolved route attempt. Keep policy denial behavior unchanged.

**Tech Stack:** Rust, `codel00p-providers`, route resolution tests, fallback routing tests, Markdown docs, `pnpm verify`.

---

### Task 1: Add Failing Route Policy Metadata Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/runtime_resolution.rs`
- Modify: `core/crates/codel00p-providers/tests/fallback_routing.rs`

- [x] **Step 1: Test resolved route policy snapshot**

Add a runtime resolution test that configures provider policy through an alias and verifies the resolved route exposes canonical policy metadata:

```rust
#[test]
fn client_resolve_reports_route_policy_metadata() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("gpt", Credential::api_key("openai-key"))
        .policy(
            ProviderPolicy::allow_all()
                .with_allowed_models("gpt", ["gpt-5-mini"])
                .with_allowed_credential_kinds("gpt", [CredentialKind::ApiKey])
                .with_required_provider_capabilities(
                    "gpt",
                    ProviderCapabilities {
                        tools: true,
                        reasoning: true,
                        ..ProviderCapabilities::default()
                    },
                )
                .with_required_model_capabilities(
                    "gpt",
                    ProviderCapabilities {
                        tools: true,
                        streaming: true,
                        ..ProviderCapabilities::default()
                    },
                ),
        )
        .build();

    let route = client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();

    assert_eq!(route.provider, "openai");
    assert_eq!(route.policy.allowed_models.as_deref(), Some(&["gpt-5-mini".to_string()][..]));
    assert_eq!(
        route.policy.allowed_credential_kinds,
        Some(vec![CredentialKind::ApiKey])
    );
    assert!(route.policy.required_provider_capabilities.tools);
    assert!(route.policy.required_provider_capabilities.reasoning);
    assert!(route.policy.required_model_capabilities.tools);
    assert!(route.policy.required_model_capabilities.streaming);
}
```

- [x] **Step 2: Test response route audit JSON includes policy**

Extend `falls_back_when_primary_route_is_rate_limited` so the successful fallback route and the failed primary attempt expose policy metadata in `codel00p_route`:

```rust
assert_eq!(
    route["selected"]["policy"]["allowed_models"],
    json!(["local-model"])
);
assert_eq!(
    route["selected"]["policy"]["allowed_credential_kinds"],
    json!(["ApiKey"])
);
assert_eq!(
    route["selected"]["policy"]["required_provider_capabilities"]["tools"],
    json!(true)
);
assert_eq!(
    route["attempts"][0]["policy"]["allowed_models"],
    json!(["anthropic/claude-sonnet"])
);
assert_eq!(
    route["attempts"][0]["policy"]["required_model_capabilities"]["reasoning"],
    json!(true)
);
```

- [x] **Step 3: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test runtime_resolution route_policy_metadata && cargo test -p codel00p-providers --test fallback_routing falls_back_when_primary_route_is_rate_limited
```

Expected: compile failure because `ResolvedInferenceRoute::policy`, `ProviderRoutePolicy`, and route audit `policy` JSON do not exist.

### Task 2: Implement Route Policy Metadata

**Files:**
- Modify: `core/crates/codel00p-providers/src/runtime.rs`
- Modify: `core/crates/codel00p-providers/src/policy.rs`
- Modify: `core/crates/codel00p-providers/src/client.rs`
- Modify: `core/crates/codel00p-providers/src/lib.rs`

- [x] **Step 1: Add `ProviderRoutePolicy`**

Add a route-specific policy metadata struct to `runtime.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ProviderRoutePolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_models: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_credential_kinds: Option<Vec<CredentialKind>>,
    #[serde(default, skip_serializing_if = "ProviderCapabilities::is_empty")]
    pub required_provider_capabilities: ProviderCapabilities,
    #[serde(default, skip_serializing_if = "ProviderCapabilities::is_empty")]
    pub required_model_capabilities: ProviderCapabilities,
}
```

Then add `pub policy: ProviderRoutePolicy` to `ResolvedInferenceRoute`.

- [x] **Step 2: Populate route policy metadata**

Add `ProviderPolicy::route_policy(provider)` in `policy.rs` and populate the new route field in `InferenceClient::resolve`:

```rust
pub(crate) fn route_policy(&self, provider: &str) -> ProviderRoutePolicy {
    ProviderRoutePolicy {
        allowed_models: self
            .allowed_models
            .get(provider)
            .map(|models| models.iter().cloned().collect()),
        allowed_credential_kinds: self
            .allowed_credential_kinds
            .get(provider)
            .map(|kinds| kinds.iter().copied().collect()),
        required_provider_capabilities: self
            .required_provider_capabilities
            .get(provider)
            .copied()
            .unwrap_or_default(),
        required_model_capabilities: self
            .required_model_capabilities
            .get(provider)
            .copied()
            .unwrap_or_default(),
    }
}
```

- [x] **Step 3: Serialize policy in route audit JSON**

Add `"policy": route.policy` to `route_metadata(...)` so successful responses and resolved attempts carry the same safe policy snapshot as the route struct.

- [x] **Step 4: Export `ProviderRoutePolicy`**

Update `lib.rs` to export `ProviderRoutePolicy` with the other runtime metadata types.

### Task 3: Document Route Policy Metadata

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Update provider docs**

Document that resolved route metadata now includes safe route policy metadata, including allowed models, allowed credential kinds, required provider capabilities, and required model capabilities.

- [x] **Step 2: Update backlog status**

Mark route audit metadata as including safe policy snapshots, not only final decisions.

### Task 4: Verify, Commit, Push

- [x] **Step 1: Run focused checks**

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test runtime_resolution route_policy_metadata && cargo test -p codel00p-providers --test fallback_routing falls_back_when_primary_route_is_rate_limited && cargo test -p codel00p-providers --test runtime_resolution && cargo test -p codel00p-providers --test fallback_routing && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Step 2: Run repo verification**

```bash
pnpm verify
```

- [x] **Step 3: Commit and push**

Commit as `feat: add route policy metadata` and push to `origin/main`.
