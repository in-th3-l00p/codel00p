# Model Capability Flags Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add typed model capability flags to normalized provider catalog models.

**Architecture:** Keep the existing raw catalog `capabilities` labels intact for provider fidelity, and add a derived `capability_flags` field on `ProviderModel` using the existing `ProviderCapabilities` shape. Derive flags from catalog capability labels, tags, and supported input modalities without changing route resolution or transport behavior.

**Tech Stack:** Rust, serde model descriptors, `codel00p-providers`, mocked model catalog tests.

---

### Task 1: Add Red Catalog Capability Assertions

**Files:**
- Modify: `core/crates/codel00p-providers/tests/model_catalog.rs`

- [x] **Step 1: Assert typed GitHub Models capability flags**

In `list_models_normalizes_github_models_catalog`, after the raw capability
label assertion, add:

```rust
assert!(models[0].capability_flags.tools);
assert!(!models[0].capability_flags.streaming);
assert!(models[0].capability_flags.vision);
assert!(models[0].capability_flags.reasoning);
```

- [x] **Step 2: Run the focused catalog test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-providers --test model_catalog list_models_normalizes_github_models_catalog
```

Expected: FAIL because `ProviderModel` does not yet expose `capability_flags`.

### Task 2: Implement Capability Flags

**Files:**
- Modify: `core/crates/codel00p-providers/src/profile.rs`
- Modify: `core/crates/codel00p-providers/src/model_catalog.rs`
- Modify: `core/crates/codel00p-providers/src/lib.rs` if public exports need adjustment

- [x] **Step 1: Make `ProviderCapabilities` serde-friendly**

In `profile.rs`, add serde derives to `ProviderCapabilities` and add:

```rust
pub fn is_empty(&self) -> bool {
    !self.tools && !self.streaming && !self.vision && !self.reasoning
}
```

- [x] **Step 2: Add `capability_flags` to `ProviderModel`**

In `ProviderModel`, add:

```rust
#[serde(default, skip_serializing_if = "ProviderCapabilities::is_empty")]
pub capability_flags: ProviderCapabilities,
```

- [x] **Step 3: Derive flags from catalog fields**

In `ModelCatalogWireModel::into_model`, compute `capability_flags` before
constructing `ProviderModel`.

Use this matching policy:

- `tools`: raw labels or tags normalize to `tools`, `toolcalling`,
  `functioncalling`, `functioncalls`, or `functions`;
- `streaming`: raw labels or tags normalize to `streaming` or `stream`;
- `vision`: raw labels or tags normalize to `vision`, `multimodal`, or
  `imageinput`, or supported input modalities include `image`;
- `reasoning`: raw labels or tags normalize to `reasoning` or `thinking`.

- [x] **Step 4: Run the focused catalog test to verify pass**

Run:

```bash
cd core
cargo test -p codel00p-providers --test model_catalog list_models_normalizes_github_models_catalog
```

Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/product-roadmap.md`

- [x] **Step 1: Document typed model capability flags**

Mention that `ProviderModel` preserves raw provider capability labels and adds
typed `capability_flags` for common tools, streaming, vision, and reasoning
checks.

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
git add docs/superpowers/plans/2026-06-10-model-capability-flags.md core/crates/codel00p-providers docs
git commit -m "feat: add model capability flags"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
