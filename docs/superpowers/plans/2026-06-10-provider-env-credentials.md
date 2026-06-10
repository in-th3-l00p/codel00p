# Provider Environment Credentials Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let provider clients load credentials from approved environment variables while keeping resolved route metadata safe and inspectable.

**Architecture:** Add `InferenceClientBuilder::credentials_from_env()` that scans the active `ProviderRegistry`, resolves API-key and AWS SigV4 provider credentials from environment variables, and stores a safe source label with each credential. Keep explicitly injected credentials higher priority than environment-loaded credentials.

**Tech Stack:** Rust, serde-free provider internals, `codel00p-providers`, env-locked runtime resolution tests.

---

### Task 1: Add Red Environment Credential Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/runtime_resolution.rs`

- [ ] **Step 1: Add env lock helper**

Add a local `with_env_lock` helper that removes the provider credential
environment variables before and after each env-based test.

- [ ] **Step 2: Test API-key env loading**

Set `CODEL00P_PROVIDER_OPENAI_API_KEY`, build a client with
`credentials_from_env()`, resolve an OpenAI request, and assert:

```rust
assert_eq!(
    route.credential_source.as_deref(),
    Some("environment:CODEL00P_PROVIDER_OPENAI_API_KEY")
);
```

- [ ] **Step 3: Test explicit credential precedence**

Set `CODEL00P_PROVIDER_OPENAI_API_KEY`, also call
`credential("openai", Credential::api_key("manual"))`, and assert the resolved
route keeps `credential_source == Some("configured")`.

- [ ] **Step 4: Test AWS SigV4 env loading**

Set `CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID`,
`CODEL00P_PROVIDER_AWS_SECRET_ACCESS_KEY`, and `CODEL00P_PROVIDER_AWS_REGION`,
build a client with `credentials_from_env()`, resolve a Bedrock request, and
assert:

```rust
assert_eq!(
    route.credential_source.as_deref(),
    Some("environment:CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID")
);
```

- [ ] **Step 5: Run tests to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-providers --test runtime_resolution
```

Expected: FAIL because `credentials_from_env` does not exist.

### Task 2: Implement Env Credential Loading

**Files:**
- Modify: `core/crates/codel00p-providers/src/client.rs`
- Modify: `core/crates/codel00p-providers/src/registry.rs`

- [ ] **Step 1: Store credential source metadata**

Replace the client and builder credential maps with a private stored credential
type containing:

```rust
struct StoredCredential {
    credential: Credential,
    source: String,
}
```

Use source `"configured"` for `InferenceClientBuilder::credential`.

- [ ] **Step 2: Add registry profile iteration**

Add `ProviderRegistry::profiles(&self) -> impl Iterator<Item = &ProviderProfile>`
so the builder can scan the active registry.

- [ ] **Step 3: Add provider-specific env vars**

Add `CODEL00P_PROVIDER_*` environment variables before generic provider env
vars in default provider profiles.

- [ ] **Step 4: Implement API-key env loading**

For API-key-like providers, read the first non-empty `profile.env_vars` value
and insert it only when no explicit credential exists for the canonical
provider ID.

- [ ] **Step 5: Implement AWS SigV4 env loading**

For `AuthType::AwsSdk`, read codel00p-specific AWS variables first, then
generic AWS variables, and insert `Credential::aws_sigv4(...)` when access key,
secret key, and region are all present.

- [ ] **Step 6: Preserve route behavior**

Update `credential_for_route`, `list_models`, and `resolve` so transports still
receive `&Credential`, while `ResolvedInferenceRoute.credential_source` exposes
the stored safe source string.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/product-roadmap.md`

- [ ] **Step 1: Document environment credentials**

Mention `credentials_from_env()` and source metadata in provider docs.

- [ ] **Step 2: Run provider verification**

Run:

```bash
cd core
cargo fmt --all
cargo test -p codel00p-providers --test runtime_resolution
cargo test -p codel00p-providers
cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

Expected: all commands pass.

- [ ] **Step 3: Run repo verification**

Run:

```bash
pnpm verify
```

Expected: PASS.

- [ ] **Step 4: Commit and push**

Run:

```bash
git add docs/superpowers/plans/2026-06-10-provider-env-credentials.md core/crates/codel00p-providers docs
git commit -m "feat: add provider env credential loading"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
