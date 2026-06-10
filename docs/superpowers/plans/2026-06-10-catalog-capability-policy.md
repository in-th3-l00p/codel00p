# Catalog Capability Policy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow provider policy to filter model catalog results by required typed model capabilities.

**Architecture:** Extend `ProviderPolicy` with per-provider required `ProviderCapabilities` for catalog filtering. Keep route-time model checks unchanged because route requests do not carry catalog model descriptors; apply capability requirements only inside `filter_models` where `ProviderModel.capability_flags` is available.

**Tech Stack:** Rust, `codel00p-providers`, mocked catalog tests, existing `ProviderCapabilities` flags.

---

### Task 1: Add Red Catalog Policy Test

**Files:**
- Modify: `core/crates/codel00p-providers/tests/model_catalog.rs`

- [x] **Step 1: Import `ProviderCapabilities`**

Update the test import to include `ProviderCapabilities`.

- [x] **Step 2: Add a capability filter test**

Add this test after `list_models_filters_disallowed_models`:

```rust
#[tokio::test]
async fn list_models_filters_by_required_capabilities() {
    let server = MockServer::start_async().await;
    let catalog = server
        .mock_async(|when, then| {
            when.method(GET).path("/models");
            then.status(200).json_body(json!({
                "data": [
                    {
                        "id": "tool-vision-model",
                        "capabilities": ["tool-calling"],
                        "supported_input_modalities": ["text", "image"]
                    },
                    {
                        "id": "tool-text-model",
                        "capabilities": ["tool-calling"],
                        "supported_input_modalities": ["text"]
                    },
                    {
                        "id": "vision-only-model",
                        "supported_input_modalities": ["text", "image"]
                    }
                ]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .policy(ProviderPolicy::allow_all().with_required_model_capabilities(
            "custom",
            ProviderCapabilities {
                tools: true,
                vision: true,
                ..ProviderCapabilities::default()
            },
        ))
        .build();

    let models = client
        .list_models(
            ModelCatalogRequest::builder("custom")
                .base_url(server.base_url())
                .build(),
        )
        .await
        .unwrap();

    catalog.assert_async().await;
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "tool-vision-model");
}
```

- [x] **Step 3: Run the focused test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-providers --test model_catalog list_models_filters_by_required_capabilities
```

Expected: FAIL because `ProviderPolicy::with_required_model_capabilities` does
not exist yet.

### Task 2: Implement Capability Policy Filtering

**Files:**
- Modify: `core/crates/codel00p-providers/src/policy.rs`

- [x] **Step 1: Store capability requirements**

Add this field to `ProviderPolicy`:

```rust
required_model_capabilities: BTreeMap<String, ProviderCapabilities>,
```

Initialize it in `allow_all`, preserve it in `allow_only`, and canonicalize
provider keys in `canonicalize`.

- [x] **Step 2: Add builder method**

Add:

```rust
pub fn with_required_model_capabilities(
    mut self,
    provider: impl Into<String>,
    capabilities: ProviderCapabilities,
) -> Self {
    self.required_model_capabilities
        .insert(provider.into(), capabilities);
    self
}
```

- [x] **Step 3: Apply capability filtering**

Update `filter_models` so it first applies existing allowlist filtering and
then filters by `required_model_capabilities` when present.

Add this helper:

```rust
fn capabilities_satisfy(model: &ProviderCapabilities, required: &ProviderCapabilities) -> bool {
    (!required.tools || model.tools)
        && (!required.streaming || model.streaming)
        && (!required.vision || model.vision)
        && (!required.reasoning || model.reasoning)
}
```

- [x] **Step 4: Run the focused test to verify pass**

Run:

```bash
cd core
cargo test -p codel00p-providers --test model_catalog list_models_filters_by_required_capabilities
```

Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Document capability policy filtering**

Mention that `ProviderPolicy` can filter catalog results by required typed model
capabilities while preserving explicit model allowlists.

- [x] **Step 2: Run provider verification**

Run:

```bash
cd core
cargo fmt --all
cargo test -p codel00p-providers --test model_catalog
cargo test -p codel00p-providers
cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

Expected: all commands pass.

- [x] **Step 3: Run repo verification**

Run:

```bash
pnpm verify
```

Expected: PASS.

- [ ] **Step 4: Commit and push**

Run:

```bash
git add docs/superpowers/plans/2026-06-10-catalog-capability-policy.md core/crates/codel00p-providers docs
git commit -m "feat: filter catalogs by model capabilities"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
