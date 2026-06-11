# Auth Type Audit Metadata Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface provider `AuthType` in safe route and catalog audit metadata.

**Architecture:** `ProviderProfile` already carries `auth_type`, while route/catalog audit surfaces currently expose only resolved credential source and kind. Add `auth_type` to `ResolvedInferenceRoute` and `ProviderModelCatalog`, populate both from the resolved profile, and include `auth_type` in response `codel00p_route` JSON. This keeps credential values hidden while making provider auth expectations inspectable.

**Tech Stack:** Rust, `codel00p-providers`, runtime resolution tests, fallback routing tests, model catalog tests, Markdown docs, `pnpm verify`.

---

### Task 1: Add Failing Auth Type Metadata Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/runtime_resolution.rs`
- Modify: `core/crates/codel00p-providers/tests/fallback_routing.rs`
- Modify: `core/crates/codel00p-providers/tests/model_catalog.rs`

- [x] **Step 1: Test resolved route auth type**

Extend route resolution coverage to assert profile auth type metadata:

```rust
assert_eq!(route.auth_type, AuthType::ApiKey);
```

Add a separate route case for GitHub Copilot if needed:

```rust
let github_route = client
    .resolve(
        &InferenceRequest::builder("github", "gpt-4o-mini-2024-07-18")
            .message(ChatMessage::user("hello"))
            .build(),
    )
    .unwrap();
assert_eq!(github_route.auth_type, AuthType::GitHubCopilot);
```

- [x] **Step 2: Test response route audit JSON auth type**

Extend `falls_back_when_primary_route_is_rate_limited` to assert:

```rust
assert_eq!(route["selected"]["auth_type"], json!("Custom"));
assert_eq!(route["attempts"][0]["auth_type"], json!("ApiKey"));
```

- [x] **Step 3: Test catalog auth type metadata**

Extend model catalog coverage to assert:

```rust
assert_eq!(result.auth_type, AuthType::Custom);
```

- [x] **Step 4: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test runtime_resolution auth_type && cargo test -p codel00p-providers --test fallback_routing falls_back_when_primary_route_is_rate_limited && cargo test -p codel00p-providers --test model_catalog auth_type
```

Expected: compile failure because `ResolvedInferenceRoute::auth_type` and `ProviderModelCatalog::auth_type` do not exist, and route audit JSON has no `auth_type`.

### Task 2: Implement Auth Type Metadata

**Files:**
- Modify: `core/crates/codel00p-providers/src/runtime.rs`
- Modify: `core/crates/codel00p-providers/src/model_catalog.rs`
- Modify: `core/crates/codel00p-providers/src/client.rs`

- [x] **Step 1: Add route auth type field**

Add `pub auth_type: AuthType` to `ResolvedInferenceRoute` and populate it from `profile.auth_type` during `InferenceClient::resolve`.

- [x] **Step 2: Add catalog auth type field**

Add `pub auth_type: AuthType` to `ProviderModelCatalog` and populate it from `profile.auth_type` during `InferenceClient::list_model_catalog`.

- [x] **Step 3: Add response route JSON auth type**

Add `"auth_type": format!("{:?}", route.auth_type)` to `route_metadata(...)`.

### Task 3: Document Auth Type Metadata

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Update provider docs**

Document that route and catalog audit metadata include provider auth type in addition to credential source and kind.

- [x] **Step 2: Update backlog status**

Mark route and catalog audit metadata as including provider auth type labels.

### Task 4: Verify, Commit, Push

- [x] **Step 1: Run focused checks**

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test runtime_resolution auth_type && cargo test -p codel00p-providers --test fallback_routing falls_back_when_primary_route_is_rate_limited && cargo test -p codel00p-providers --test model_catalog auth_type && cargo test -p codel00p-providers --test runtime_resolution && cargo test -p codel00p-providers --test fallback_routing && cargo test -p codel00p-providers --test model_catalog && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Step 2: Run repo verification**

```bash
pnpm verify
```

- [x] **Step 3: Commit and push**

Commit as `feat: add auth type audit metadata` and push to `origin/main`.
