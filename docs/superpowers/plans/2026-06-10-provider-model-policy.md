# Provider Model Policy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add model-level allowlist policy on top of existing provider allowlists.

**Architecture:** `ProviderPolicy` remains the single policy object. Existing provider allowlists continue to work, and model allowlists are keyed by canonical provider ID after builder canonicalization. Route resolution rejects disallowed models before inference; model catalog listing returns only policy-allowed models when a model allowlist is configured.

**Tech Stack:** Rust, `codel00p-providers`, `httpmock`, policy unit tests, `cargo test`, `cargo clippy`, `pnpm verify`.

---

### Task 1: Add Failing Policy Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/provider_policy.rs`
- Modify: `core/crates/codel00p-providers/tests/model_catalog.rs`

- [x] **Step 1: Test model policy blocks disallowed model**

Create a client with:

```rust
ProviderPolicy::allow_all()
    .with_allowed_models("openrouter", ["anthropic/claude-sonnet"])
```

Resolve a request for `openrouter` model `openai/gpt-test`. Assert
`ProviderError::PolicyDenied` with provider `openrouter` and a reason containing
`model is not allowed`.

- [x] **Step 2: Test model policy accepts allowed aliases**

Create a client with:

```rust
ProviderPolicy::allow_all()
    .with_allowed_models("or", ["anthropic/claude-sonnet"])
```

Resolve a request for canonical provider `openrouter` model
`anthropic/claude-sonnet`. Assert route provider is `openrouter`.

- [x] **Step 3: Test catalog listing filters disallowed models**

Mock a catalog returning two models. Configure model policy to allow only one.
Assert `list_models` returns the allowed model only.

- [x] **Step 4: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test provider_policy
cd core && cargo test -p codel00p-providers --test model_catalog
```

Expected: compile failure because `ProviderPolicy::with_allowed_models` does
not exist.

### Task 2: Implement Model Allowlists

**Files:**
- Modify: `core/crates/codel00p-providers/src/policy.rs`
- Modify: `core/crates/codel00p-providers/src/client.rs`

- [x] **Step 1: Extend `ProviderPolicy`**

Add:

```rust
allowed_models: BTreeMap<String, BTreeSet<String>>
```

and a public builder method:

```rust
pub fn with_allowed_models<I, S>(self, provider: impl Into<String>, models: I) -> Self
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
```

- [x] **Step 2: Canonicalize provider keys**

During `InferenceClientBuilder::build`, canonicalize model policy provider keys
through the provider registry the same way provider allowlists are canonicalized.

- [x] **Step 3: Enforce model policy during route resolution**

After provider policy passes in `InferenceClient::resolve`, call a new
`ProviderPolicy::check_model(provider, model)` helper.

- [x] **Step 4: Filter model catalogs**

After `list_models` parses a catalog, filter returned models through
`ProviderPolicy::filter_models(provider, models)`.

### Task 3: Document, Verify, Commit, Push

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/agentic-backlog.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/superpowers/plans/2026-06-10-provider-model-policy.md`

- [x] **Step 1: Document model policy**

Mention provider-level and model-level allowlists in provider docs and roadmap.

- [x] **Step 2: Run targeted provider checks**

Run:

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test provider_policy && cargo test -p codel00p-providers --test model_catalog && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Step 3: Run repo verification**

Run:

```bash
pnpm verify
```

- [x] **Step 4: Commit and push**

Run:

```bash
git add core/crates/codel00p-providers/src/policy.rs core/crates/codel00p-providers/src/client.rs core/crates/codel00p-providers/tests/provider_policy.rs core/crates/codel00p-providers/tests/model_catalog.rs core/crates/codel00p-providers/README.md docs/providers.md docs/agentic-backlog.md docs/product-roadmap.md docs/roadmap.md docs/superpowers/plans/2026-06-10-provider-model-policy.md
git commit -m "feat: add provider model policy"
git push origin main
```
