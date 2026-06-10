# Provider Cloud Proxy Routing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let organization-managed clients route provider requests through a configured cloud proxy without changing provider IDs, model IDs, messages, tools, or provider response normalization.

**Architecture:** Add a client-level proxy route keyed by provider. Request-level base URL overrides remain the highest-priority explicit route; otherwise a configured provider proxy supplies the base URL and credential before falling back to the provider default. Route metadata exposes the proxy as a safe value source and marks the credential source without exposing the credential.

**Tech Stack:** Rust, `codel00p-providers`, route-resolution tests, mocked Chat Completions tests.

---

### Task 1: Add Red Proxy Routing Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/runtime_resolution.rs`
- Modify: `core/crates/codel00p-providers/tests/chat_completions_transport.rs`

- [ ] **Step 1: Test route resolution**

Build a client with a provider proxy for `openai`. Assert `resolve(...)`
returns the proxy base URL, a proxy route source, and a cloud-proxy credential
source.

- [ ] **Step 2: Test explicit request override precedence**

Configure a provider proxy and send a request with `.base_url(...)`. Assert the
request override still wins and route metadata reports `RequestOverride`.

- [ ] **Step 3: Test transport uses proxy credential**

Mock a custom Chat Completions endpoint, configure a provider proxy for
`custom`, and assert the request is sent to the proxy with the proxy bearer
token even when no direct provider credential exists.

- [ ] **Step 4: Run tests to verify failure**

Run:

```bash
cargo test -p codel00p-providers --test runtime_resolution
cargo test -p codel00p-providers --test chat_completions_transport cloud_proxy_routes_chat_completions_through_configured_gateway
```

Expected: FAIL because `InferenceClientBuilder::provider_proxy` and the proxy
route source do not exist.

### Task 2: Implement Client Proxy Routes

**Files:**
- Modify: `core/crates/codel00p-providers/src/client.rs`
- Modify: `core/crates/codel00p-providers/src/runtime.rs`

- [ ] **Step 1: Add proxy route storage**

Add an internal provider proxy map to `InferenceClient` and
`InferenceClientBuilder`.

- [ ] **Step 2: Add builder method**

Add:

```rust
pub fn provider_proxy(
    mut self,
    provider: impl Into<String>,
    base_url: impl Into<String>,
    credential: Credential,
) -> Self
```

- [ ] **Step 3: Canonicalize proxy keys**

Canonicalize proxy provider keys during `build()`, matching credentials,
pricing, and policy behavior.

- [ ] **Step 4: Resolve proxy routes**

Use route order: request base URL override, client provider proxy, provider
default. Mark proxy routes with a new route value source and credential source.

- [ ] **Step 5: Use proxy credentials**

When a route uses a provider proxy, send the proxy credential to the existing
transport instead of requiring a direct provider credential.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/product-roadmap.md`

- [ ] **Step 1: Document proxy routing**

Explain provider proxy routing, request override precedence, and safe route
metadata.

- [ ] **Step 2: Run targeted verification**

Run:

```bash
cargo fmt --all
cargo test -p codel00p-providers --test runtime_resolution
cargo test -p codel00p-providers --test chat_completions_transport cloud_proxy_routes_chat_completions_through_configured_gateway
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
git add docs/superpowers/plans/2026-06-10-provider-cloud-proxy-routing.md core/crates/codel00p-providers docs
git commit -m "feat: add provider cloud proxy routing"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
