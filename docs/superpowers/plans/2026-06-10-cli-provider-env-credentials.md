# CLI Provider Env Credentials Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Have CLI-built provider clients use the shared provider environment credential resolver so resolved route metadata preserves safe environment source labels.

**Architecture:** Promote provider environment credential lookup to a registry-facing API in `codel00p-providers`. Update `codel00p-cli` provider construction to validate credentials through that shared API and build clients with `credentials_from_env()`, avoiding duplicate CLI parsing and preserving source metadata.

**Tech Stack:** Rust, `codel00p-providers`, `codel00p-cli`, env-locked CLI unit tests.

---

### Task 1: Add Red CLI Source Test

**Files:**
- Modify: `core/crates/codel00p-cli/src/providers.rs`

- [ ] **Step 1: Add env lock helper in tests**

Add a small test helper that removes `CODEL00P_PROVIDER_OPENAI_API_KEY` and
`OPENAI_API_KEY` before and after the test.

- [ ] **Step 2: Assert CLI-built route source**

Set `CODEL00P_PROVIDER_OPENAI_API_KEY`, build the CLI provider client with
`build_provider_client("openai")`, resolve an OpenAI request, and assert:

```rust
assert_eq!(
    route.credential_source.as_deref(),
    Some("environment:CODEL00P_PROVIDER_OPENAI_API_KEY")
);
```

- [ ] **Step 3: Run test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-cli providers::tests::build_provider_client_preserves_env_credential_source
```

Expected: FAIL because CLI currently reinjects the env credential as
`configured`.

### Task 2: Expose Shared Env Credential Lookup

**Files:**
- Modify: `core/crates/codel00p-providers/src/credentials.rs`
- Modify: `core/crates/codel00p-providers/src/client.rs`
- Modify: `core/crates/codel00p-providers/src/registry.rs`
- Modify: `core/crates/codel00p-providers/src/lib.rs`

- [ ] **Step 1: Add public resolved credential type**

Add a public `ResolvedProviderCredential` with `credential: Credential` and
`source: String`.

- [ ] **Step 2: Use resolved credential internally**

Replace the private stored credential shape with the public
`ResolvedProviderCredential` inside `InferenceClient` and
`InferenceClientBuilder`.

- [ ] **Step 3: Add registry env lookup**

Add `ProviderRegistry::credential_from_env(&self, provider: &str) ->
Option<ResolvedProviderCredential>` that resolves aliases, handles API key and
AWS SigV4 providers, and records the safe source label.

- [ ] **Step 4: Keep builder behavior**

Update `InferenceClientBuilder::credentials_from_env()` to use
`ProviderRegistry::credential_from_env` for each profile.

### Task 3: Switch CLI Construction

**Files:**
- Modify: `core/crates/codel00p-cli/src/providers.rs`
- Modify: `core/crates/codel00p-cli/README.md`

- [ ] **Step 1: Remove duplicate CLI secret parsing**

Delete the CLI-local `provider_credential`, `read_secret`, `is_bedrock`,
`aws_sigv4_credential`, and `read_first_secret` helpers.

- [ ] **Step 2: Delegate env var lists to registry profiles**

Make `provider_env_vars(provider)` return `profile.env_vars.to_vec()` from
`default_registry().resolve(provider)`.

- [ ] **Step 3: Validate with shared resolver**

Make `build_provider_client` call
`registry.credential_from_env(provider).is_some()` for its friendly missing
credential error, then return a client built with:

```rust
InferenceClient::builder()
    .registry(registry)
    .credentials_from_env()
    .build()
```

- [ ] **Step 4: Document CLI env source behavior**

Mention that CLI provider clients use the shared environment resolver and keep
safe source labels in route metadata.

### Task 4: Verification And Commit

**Files:**
- Modify: `docs/product-roadmap.md`

- [ ] **Step 1: Run focused verification**

Run:

```bash
cd core
cargo fmt --all
cargo test -p codel00p-cli providers::tests
cargo test -p codel00p-providers --test runtime_resolution
cargo clippy -p codel00p-cli --all-targets -- -D warnings
cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

Expected: all commands pass.

- [ ] **Step 2: Run repo verification**

Run:

```bash
pnpm verify
```

Expected: PASS.

- [ ] **Step 3: Commit and push**

Run:

```bash
git add docs/superpowers/plans/2026-06-10-cli-provider-env-credentials.md core/crates/codel00p-providers core/crates/codel00p-cli docs
git commit -m "feat: reuse provider env credentials in cli"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
