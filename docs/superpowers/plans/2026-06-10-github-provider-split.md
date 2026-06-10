# GitHub Provider Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split GitHub Copilot and GitHub Models into distinct provider profiles while preserving the existing `github` Copilot route.

**Architecture:** Keep provider differences in registry metadata and existing Chat Completions transport behavior. `github` remains the Copilot-compatible profile using `api.githubcopilot.com` and `max_completion_tokens`; `github-models` becomes the official GitHub Models profile using `models.github.ai/inference`, `max_tokens`, and the GitHub catalog endpoint.

**Tech Stack:** Rust workspace, `codel00p-providers`, mocked HTTP tests with `httpmock`, CLI env resolution, Markdown provider docs.

---

### Task 1: Lock The Registry Contract

**Files:**
- Modify: `core/crates/codel00p-providers/tests/provider_registry.rs`

- [ ] **Step 1: Write the failing registry test**

Add expectations that `github` resolves to "GitHub Copilot", aliases `github-copilot` and `copilot` resolve to `github`, and aliases `github-models`, `github-model`, and `gh-models` resolve to the new canonical `github-models` provider.

- [ ] **Step 2: Run the registry test to verify it fails**

Run: `cargo test -p codel00p-providers --test provider_registry`
Expected: FAIL because the current registry has no `github-models` profile and still names `github` as "GitHub Models".

- [ ] **Step 3: Implement registry metadata**

Modify `core/crates/codel00p-providers/src/registry.rs` so:

```rust
ProviderProfile {
    id: "github",
    aliases: &["github-copilot", "copilot"],
    display_name: "GitHub Copilot",
    description: "GitHub Copilot Chat Completions-compatible endpoint",
    api_mode: ApiMode::ChatCompletions,
    auth_type: AuthType::GitHubCopilot,
    env_vars: &["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"],
    default_base_url: Some("https://api.githubcopilot.com"),
    models_url: None,
    default_aux_model: None,
    output_token_parameter: OutputTokenParameter::MaxCompletionTokens,
    capabilities: ProviderCapabilities::agentic(),
}
```

and add:

```rust
ProviderProfile {
    id: "github-models",
    aliases: &["github-model", "gh-models"],
    display_name: "GitHub Models",
    description: "GitHub Models OpenAI-compatible inference endpoint",
    api_mode: ApiMode::ChatCompletions,
    auth_type: AuthType::ApiKey,
    env_vars: &[
        "CODEL00P_PROVIDER_GITHUB_MODELS_TOKEN",
        "GITHUB_TOKEN",
        "GH_TOKEN",
    ],
    default_base_url: Some("https://models.github.ai/inference"),
    models_url: Some("https://models.github.ai/catalog/models"),
    default_aux_model: None,
    output_token_parameter: OutputTokenParameter::MaxTokens,
    capabilities: ProviderCapabilities::agentic(),
}
```

- [ ] **Step 4: Run the registry test to verify it passes**

Run: `cargo test -p codel00p-providers --test provider_registry`
Expected: PASS.

### Task 2: Prove GitHub Models Request Shape

**Files:**
- Modify: `core/crates/codel00p-providers/tests/chat_completions_transport.rs`

- [ ] **Step 1: Write the failing transport test**

Add a mocked Chat Completions test for provider `github-models` that sets `base_url` to `<server>/inference`, expects `POST /inference/chat/completions`, bearer auth, `model`, and `max_tokens`.

- [ ] **Step 2: Run the transport test to verify it fails**

Run: `cargo test -p codel00p-providers --test chat_completions_transport github_models_uses_models_endpoint_and_max_tokens`
Expected: FAIL because `github-models` is not registered yet.

- [ ] **Step 3: Run after registry implementation**

Run the same command after Task 1 implementation.
Expected: PASS with no transport-specific changes.

### Task 3: Parse GitHub Models Catalogs

**Files:**
- Modify: `core/crates/codel00p-providers/tests/model_catalog.rs`
- Modify: `core/crates/codel00p-providers/src/model_catalog.rs`

- [ ] **Step 1: Write the failing catalog test**

Add a mocked `GET /catalog/models` test for provider `github-models` returning a top-level JSON array. Assert `name` maps to `display_name`, `publisher` maps to `owned_by`, and extra GitHub fields remain in `provider_data`.

- [ ] **Step 2: Run the catalog test to verify it fails**

Run: `cargo test -p codel00p-providers --test model_catalog list_models_normalizes_github_models_catalog`
Expected: FAIL because the parser only accepts `{ "data": [...] }`.

- [ ] **Step 3: Implement untagged catalog parsing**

Represent the wire response as an untagged enum accepting both `{ data: Vec<_> }` and `Vec<_>`. Add optional `publisher` handling in `ModelCatalogWireModel::into_model`.

- [ ] **Step 4: Run the catalog test to verify it passes**

Run: `cargo test -p codel00p-providers --test model_catalog list_models_normalizes_github_models_catalog`
Expected: PASS.

### Task 4: Split Credential Environment Resolution

**Files:**
- Modify: `core/crates/codel00p-providers/tests/integration_config.rs`
- Modify: `core/crates/codel00p-providers/tests/support/mod.rs`
- Modify: `core/crates/codel00p-harness/tests/integration_config.rs`
- Modify: `core/crates/codel00p-harness/tests/support/mod.rs`
- Modify: `core/crates/codel00p-cli/src/providers.rs`

- [ ] **Step 1: Write failing env tests**

Add tests proving `github-models` prefers `CODEL00P_PROVIDER_GITHUB_MODELS_TOKEN` before generic GitHub tokens while `github` continues to prefer Copilot-specific variables.

- [ ] **Step 2: Run env tests to verify they fail**

Run:

```bash
cargo test -p codel00p-providers --test integration_config
cargo test -p codel00p-harness --test integration_config
```

Expected: FAIL because `github-models` has no env resolution.

- [ ] **Step 3: Implement env resolution**

Add `github-models | github-model | gh-models` branches that return:

```rust
vec![
    "CODEL00P_PROVIDER_GITHUB_MODELS_TOKEN",
    "GITHUB_TOKEN",
    "GH_TOKEN",
]
```

- [ ] **Step 4: Run env tests to verify they pass**

Run the same test commands.
Expected: PASS.

### Task 5: Update Docs And Verify

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `core/crates/codel00p-cli/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/product-roadmap.md`

- [ ] **Step 1: Update docs**

Document the split IDs: `github` for Copilot and `github-models` for official GitHub Models. Mention the GitHub Models base URL, catalog URL, and env variable priority.

- [ ] **Step 2: Run targeted provider verification**

Run:

```bash
cargo fmt --all
cargo test -p codel00p-providers --test provider_registry
cargo test -p codel00p-providers --test chat_completions_transport github_models_uses_models_endpoint_and_max_tokens
cargo test -p codel00p-providers --test model_catalog list_models_normalizes_github_models_catalog
cargo test -p codel00p-providers --test integration_config
cargo test -p codel00p-harness --test integration_config
cargo test -p codel00p-providers
cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

Expected: all commands pass.

- [ ] **Step 3: Run repo verification**

Run: `pnpm verify`
Expected: PASS.

- [ ] **Step 4: Commit and push**

Run:

```bash
git add docs/superpowers/plans/2026-06-10-github-provider-split.md core docs
git commit -m "feat: split github models provider"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
