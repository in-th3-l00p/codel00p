# Provider Route Intelligence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add safe route intelligence metadata to provider resolution so CLI, desktop, cloud, and audit surfaces can explain provider choices before fallback routing is introduced.

**Architecture:** Keep the first slice additive. `InferenceClient::resolve` will continue returning `ResolvedInferenceRoute`, but the route will also expose base URL source, provider capabilities, model catalog URL, and policy decision metadata. Transports remain unchanged.

**Tech Stack:** Rust, `codel00p-providers`, provider route tests, `cargo test`, `cargo clippy`, `pnpm verify`.

---

### Task 1: Add Route Intelligence Tests

**Files:**
- Modify: `core/crates/codel00p-providers/tests/runtime_resolution.rs`

- [x] **Step 1: Write failing tests**

Add assertions that resolved routes expose:

- `base_url_source == ProviderDefault` when using a provider default;
- `base_url_source == RequestOverride` when a request overrides the URL;
- `policy_decision == Allowed` for an allowed route;
- provider `capabilities` copied from the profile;
- `models_url` copied from the profile.

- [x] **Step 2: Verify red state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test runtime_resolution
```

Expected: compile failure because route intelligence fields and enums are not implemented yet.

### Task 2: Implement Route Intelligence Fields

**Files:**
- Modify: `core/crates/codel00p-providers/src/runtime.rs`
- Modify: `core/crates/codel00p-providers/src/client.rs`
- Modify: `core/crates/codel00p-providers/src/lib.rs`

- [x] **Step 1: Add public route metadata enums**

Add:

- `RouteValueSource::{RequestOverride, ProviderDefault}`;
- `ProviderPolicyDecision::Allowed`.

- [x] **Step 2: Extend `ResolvedInferenceRoute`**

Add:

- `base_url_source: RouteValueSource`;
- `policy_decision: ProviderPolicyDecision`;
- `capabilities: ProviderCapabilities`;
- `models_url: Option<String>`.

- [x] **Step 3: Populate fields in `InferenceClient::resolve`**

Resolve base URL source before choosing the base URL. Populate capabilities,
models URL, and allowed policy decision from the provider profile and policy
check.

- [x] **Step 4: Verify green state**

Run:

```bash
cd core && cargo test -p codel00p-providers --test runtime_resolution
```

Expected: all runtime resolution tests pass.

### Task 3: Document The New Route Intelligence

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Update provider docs**

Document that route inspection now carries safe audit metadata for base URL
source, capabilities, model catalog URL, and policy decision.

- [x] **Step 2: Update backlog status**

Mark route audit metadata as started and leave fallback routing as the next
provider-platform item.

### Task 4: Verify, Commit, Push

**Files:**
- All files above.

- [x] **Step 1: Run targeted provider checks**

Run:

```bash
cd core && cargo fmt --all && cargo test -p codel00p-providers --test runtime_resolution && cargo test -p codel00p-providers && cargo clippy -p codel00p-providers --all-targets -- -D warnings
```

- [x] **Step 2: Run repo verification**

Run:

```bash
pnpm verify
```

- [x] **Step 3: Commit and push**

Run:

```bash
git add core/crates/codel00p-providers/src/runtime.rs core/crates/codel00p-providers/src/client.rs core/crates/codel00p-providers/src/lib.rs core/crates/codel00p-providers/tests/runtime_resolution.rs core/crates/codel00p-providers/README.md docs/providers.md docs/agentic-backlog.md docs/superpowers/plans/2026-06-10-provider-route-intelligence.md
git commit -m "feat: add provider route intelligence"
git push origin main
```
