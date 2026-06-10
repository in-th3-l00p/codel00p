# Provider Catalog Metadata Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Normalize common model catalog metadata so policy and UI surfaces can read capabilities, modalities, and token limits without provider-specific JSON digging.

**Architecture:** Extend the existing `ProviderModel` descriptor with additive fields. Preserve raw provider catalog fields in `provider_data` while also extracting common fields into typed, serializable Rust values.

**Tech Stack:** Rust, `serde`, `serde_json`, `codel00p-providers` model catalog tests.

---

### Task 1: Add Red Tests For Common Metadata

**Files:**
- Modify: `core/crates/codel00p-providers/tests/model_catalog.rs`

- [ ] **Step 1: Extend the OpenAI-compatible catalog test**

Add an assertion that `context_length` maps to `models[1].limits.max_input_tokens`.

- [ ] **Step 2: Extend the GitHub Models catalog fixture**

Add these fields to the GitHub model fixture:

```json
{
  "capabilities": ["chat", "tool-calling"],
  "limits": {
    "max_input_tokens": 128000,
    "max_output_tokens": 16384
  },
  "supported_input_modalities": ["text", "image"],
  "supported_output_modalities": ["text"]
}
```

Assert:

- `models[0].capabilities == ["chat", "tool-calling"]`;
- `models[0].limits.max_input_tokens == Some(128000)`;
- `models[0].limits.max_output_tokens == Some(16384)`;
- `models[0].input_modalities == ["text", "image"]`;
- `models[0].output_modalities == ["text"]`;
- the raw `capabilities`, `limits`, and modality fields remain in `provider_data`.

- [ ] **Step 3: Run tests to verify failure**

Run: `cargo test -p codel00p-providers --test model_catalog`
Expected: FAIL because `ProviderModel` does not expose those fields yet.

### Task 2: Implement Typed Metadata

**Files:**
- Modify: `core/crates/codel00p-providers/src/model_catalog.rs`
- Modify: `core/crates/codel00p-providers/src/lib.rs`

- [ ] **Step 1: Add `ProviderModelLimits`**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ProviderModelLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
}
```

and an `is_empty()` method for serde skipping.

- [ ] **Step 2: Extend `ProviderModel`**

Add `capabilities`, `input_modalities`, `output_modalities`, and `limits` with serde defaults and skip rules.

- [ ] **Step 3: Parse provider fields**

Read GitHub-style `capabilities`, `limits`, `supported_input_modalities`, and `supported_output_modalities` from the wire model. Also map OpenAI-style `context_length` to `limits.max_input_tokens` when no explicit input limit exists.

- [ ] **Step 4: Preserve raw provider data**

Reinsert consumed raw fields into `provider_data` so existing provider-specific consumers do not lose detail.

- [ ] **Step 5: Re-export the limits type**

Update `src/lib.rs` to re-export `ProviderModelLimits`.

- [ ] **Step 6: Run tests to verify pass**

Run: `cargo test -p codel00p-providers --test model_catalog`
Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/product-roadmap.md`

- [ ] **Step 1: Document normalized metadata**

Mention that catalog listing now exposes typed capabilities, modalities, and token limits while preserving raw provider JSON.

- [ ] **Step 2: Run targeted verification**

Run:

```bash
cargo fmt --all
cargo test -p codel00p-providers --test model_catalog
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
git add docs/superpowers/plans/2026-06-10-provider-catalog-metadata.md core/crates/codel00p-providers docs
git commit -m "feat: normalize provider catalog metadata"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
