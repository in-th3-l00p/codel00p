# GitHub Models Live Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an ignored live smoke test for the official GitHub Models provider profile.

**Architecture:** Reuse the existing `IntegrationConfig` helper and ignored live-test pattern. The test uses provider `github-models`, reads credentials through the new GitHub Models env branch, defaults the model to a low-cost GitHub Models entry, and supports `CODEL00P_PROVIDER_GITHUB_MODELS_MODEL` for teams that restrict model access.

**Tech Stack:** Rust integration tests, `codel00p-providers`, GitHub Models Chat Completions profile, Markdown docs.

---

### Task 1: Model Env Configuration

**Files:**
- Modify: `core/crates/codel00p-providers/tests/integration_config.rs`
- Modify: `core/crates/codel00p-providers/tests/support/mod.rs`

- [ ] **Step 1: Write failing config tests**

Add tests that prove `IntegrationConfig::github_models_model()` returns `openai/gpt-4o-mini` by default and reads `CODEL00P_PROVIDER_GITHUB_MODELS_MODEL` when set.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p codel00p-providers --test integration_config github_models_model`
Expected: FAIL because `github_models_model()` is not implemented.

- [ ] **Step 3: Implement model helper**

Add `const DEFAULT_GITHUB_MODELS_MODEL: &str = "openai/gpt-4o-mini";` and:

```rust
pub fn github_models_model(&self) -> String {
    read_secret("CODEL00P_PROVIDER_GITHUB_MODELS_MODEL")
        .unwrap_or_else(|| DEFAULT_GITHUB_MODELS_MODEL.to_string())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p codel00p-providers --test integration_config github_models_model`
Expected: PASS.

### Task 2: Live Smoke Test

**Files:**
- Create: `core/crates/codel00p-providers/tests/live_github_models.rs`

- [ ] **Step 1: Write the ignored live test**

Create an ignored async test that:

- builds `IntegrationConfig::from_env()`;
- calls `skip_message("github-models")` and returns early with `eprintln!` when disabled or missing credentials;
- builds `InferenceClient` with `default_registry()` and credential `github-models`;
- sends a Chat Completions request through provider `github-models`;
- uses `config.github_models_model()`;
- asks the model to reply exactly `codel00p-github-models-ok`;
- asserts the trimmed content equals that string.

- [ ] **Step 2: Run ignored test without credentials**

Run: `cargo test -p codel00p-providers --test live_github_models -- --ignored --nocapture`
Expected: PASS with a skip message when integration env is not enabled.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/product-roadmap.md`

- [ ] **Step 1: Document the live test**

Add command examples for `live_github_models`, the token variables, and `CODEL00P_PROVIDER_GITHUB_MODELS_MODEL`.

- [ ] **Step 2: Run targeted verification**

Run:

```bash
cargo fmt --all
cargo test -p codel00p-providers --test integration_config github_models_model
cargo test -p codel00p-providers --test live_github_models -- --ignored --nocapture
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
git add docs/superpowers/plans/2026-06-10-github-models-live-integration.md core/crates/codel00p-providers docs
git commit -m "test: add github models live integration"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
