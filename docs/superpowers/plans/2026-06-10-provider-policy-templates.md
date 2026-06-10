# Provider Policy Templates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a conservative built-in provider policy template for enterprise direct-provider routing.

**Architecture:** Keep `ProviderPolicy` as the only policy object. Add an additive constructor, `ProviderPolicy::enterprise_direct()`, that allows the direct corporate provider profiles and excludes broker/custom endpoints unless the caller explicitly opts into a different policy.

**Tech Stack:** Rust, `codel00p-providers`, provider policy tests, Markdown docs.

---

### Task 1: Add Red Template Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/provider_policy.rs`

- [ ] **Step 1: Test the template allows direct providers**

Create a client with `ProviderPolicy::enterprise_direct()` and resolve representative requests for:

- `openai`;
- `anthropic`;
- `azure-foundry` with a request base URL;
- `bedrock`;
- `gemini`;
- `github`;
- `github-models`.

Assert each route resolves to the canonical provider ID.

- [ ] **Step 2: Test the template blocks broker/custom providers**

With the same template, resolve `openrouter` and `custom` requests. Provide a custom base URL for `custom` so policy denial is tested instead of missing base URL. Assert `ProviderError::PolicyDenied`.

- [ ] **Step 3: Run tests to verify failure**

Run: `cargo test -p codel00p-providers --test provider_policy enterprise_direct`
Expected: FAIL because `ProviderPolicy::enterprise_direct()` does not exist.

### Task 2: Implement Template Constructor

**Files:**
- Modify: `core/crates/codel00p-providers/src/policy.rs`

- [ ] **Step 1: Add template provider list**

Add an internal constant:

```rust
const ENTERPRISE_DIRECT_PROVIDERS: &[&str] = &[
    "openai",
    "anthropic",
    "azure-foundry",
    "bedrock",
    "gemini",
    "github",
    "github-models",
];
```

- [ ] **Step 2: Add constructor**

Add:

```rust
pub fn enterprise_direct() -> Self {
    Self::allow_only(ENTERPRISE_DIRECT_PROVIDERS.iter().copied())
}
```

- [ ] **Step 3: Run tests to verify pass**

Run: `cargo test -p codel00p-providers --test provider_policy enterprise_direct`
Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/product-roadmap.md`

- [ ] **Step 1: Document the policy template**

Explain that `enterprise_direct()` allows direct first-wave providers and leaves OpenRouter/custom endpoints as explicit opt-ins.

- [ ] **Step 2: Run targeted verification**

Run:

```bash
cargo fmt --all
cargo test -p codel00p-providers --test provider_policy
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
git add docs/superpowers/plans/2026-06-10-provider-policy-templates.md core/crates/codel00p-providers docs
git commit -m "feat: add provider policy template"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
