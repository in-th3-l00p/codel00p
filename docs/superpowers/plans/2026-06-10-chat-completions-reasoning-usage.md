# Chat Completions Reasoning Usage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Normalize OpenAI-compatible Chat Completions reasoning-token usage into the provider-neutral `Usage` shape.

**Architecture:** Extend the existing Chat Completions usage parser with `completion_tokens_details.reasoning_tokens`. Keep output tokens as provider-reported completion tokens and expose reasoning as a separate normalized counter for cost estimation and audit surfaces.

**Tech Stack:** Rust, serde, `codel00p-providers`, mocked Chat Completions transport tests.

---

### Task 1: Add Red Usage Metadata Test

**Files:**
- Modify: `core/crates/codel00p-providers/tests/chat_completions_transport.rs`

- [ ] **Step 1: Extend mocked usage payload**

Add `completion_tokens_details.reasoning_tokens` to the primary Chat
Completions response fixture.

- [ ] **Step 2: Assert normalized reasoning tokens**

Assert `response.usage.unwrap().reasoning_tokens` matches the fixture value.

- [ ] **Step 3: Run test to verify failure**

Run:

```bash
cargo test -p codel00p-providers --test chat_completions_transport chat_completions_posts_openai_compatible_payload_and_normalizes_response
```

Expected: FAIL because Chat Completions usage normalization currently ignores
`completion_tokens_details`.

### Task 2: Implement Reasoning Usage Parsing

**Files:**
- Modify: `core/crates/codel00p-providers/src/transports/chat_completions.rs`

- [ ] **Step 1: Add completion token detail shape**

Add a `CompletionTokenDetails` struct with optional `reasoning_tokens`.

- [ ] **Step 2: Normalize reasoning tokens**

Read `completion_tokens_details.reasoning_tokens` into
`Usage.reasoning_tokens`, defaulting to zero when absent.

- [ ] **Step 3: Run targeted tests**

Run:

```bash
cargo fmt --all
cargo test -p codel00p-providers --test chat_completions_transport
```

Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/product-roadmap.md`

- [ ] **Step 1: Document expanded usage metadata**

Mention Chat Completions reasoning-token normalization alongside existing cache
usage metadata.

- [ ] **Step 2: Run provider verification**

Run:

```bash
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
git add docs/superpowers/plans/2026-06-10-chat-completions-reasoning-usage.md core/crates/codel00p-providers docs
git commit -m "feat: normalize chat completions reasoning usage"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
