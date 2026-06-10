# Native Agent Provider Modes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let `codel00p agent run` execute through every implemented native provider transport, not only Chat-Completions-compatible providers.

**Architecture:** The harness already talks through `ProviderModelClient`, which delegates to `codel00p-providers::InferenceClient`. The CLI only needs to stop rejecting implemented native `ApiMode` values while keeping provider lookup, credentials, base URL overrides, and test coverage intact.

**Tech Stack:** Rust workspace, `codel00p-cli`, `codel00p-harness`, `codel00p-providers`, `httpmock`, `cargo test`, `cargo clippy`, `pnpm verify`.

---

### Task 1: Add Native Provider CLI Agent Tests

**Files:**
- Modify: `core/crates/codel00p-cli/tests/agent_cli.rs`

- [x] **Step 1: Write failing tests**

Add mocked CLI agent tests for:

- OpenAI Responses: `POST /v1/responses`, bearer token from `CODEL00P_PROVIDER_OPENAI_API_KEY`;
- Anthropic Messages: `POST /v1/messages`, `x-api-key` from `CODEL00P_PROVIDER_ANTHROPIC_API_KEY`;
- Gemini GenerateContent: `POST /v1beta/models/gemini-2.5-flash:generateContent`, `x-goog-api-key` from `CODEL00P_PROVIDER_GEMINI_API_KEY`;
- AWS Bedrock Converse: `POST /model/<model>/converse`, SigV4 authorization from `CODEL00P_PROVIDER_AWS_*`.

Each test should run the real `codel00p` binary with `agent run`, a mock base URL, the native provider ID, and a final text response.

- [x] **Step 2: Verify the red state**

Run:

```bash
cd core && cargo test -p codel00p-cli --test agent_cli native_provider
```

Expected: the new tests fail with the current CLI gate that says native provider modes are not supported for agent run.

### Task 2: Enable Implemented Native Modes In CLI Provider Builder

**Files:**
- Modify: `core/crates/codel00p-cli/src/providers.rs`

- [x] **Step 1: Remove the chat-only mode gate**

Keep unknown-provider errors and credential resolution. Remove the validation that rejects `ApiMode::Responses`, `ApiMode::AnthropicMessages`, `ApiMode::BedrockConverse`, and `ApiMode::Gemini`.

- [x] **Step 2: Run the native provider tests**

Run:

```bash
cd core && cargo test -p codel00p-cli --test agent_cli native_provider
```

Expected: the new native provider CLI tests pass.

### Task 3: Update Product And Capability Docs

**Files:**
- Modify: `README.md`
- Modify: `core/crates/codel00p-cli/README.md`
- Modify: `docs/agent-capability-parity.md`
- Modify: `docs/roadmap.md`

- [x] **Step 1: Remove stale gating language**

Docs should say CLI agent runs can use Chat Completions, OpenAI Responses, Anthropic Messages, AWS Bedrock Converse, and Gemini GenerateContent through the provider layer.

- [x] **Step 2: Verify docs references**

Run:

```bash
rg -n "native provider modes remain gated|CLI enablement for those native modes|agent run currently supports chat_completions|currently enable Chat-Completions-compatible" README.md docs/*.md core/crates/codel00p-cli/README.md
```

Expected: no stale claim that implemented native modes are unavailable in CLI agent runs.

### Task 4: Full Verification And Commit

**Files:**
- All files modified above.

- [x] **Step 1: Format and run targeted checks**

Run:

```bash
cd core && cargo fmt --all && cargo test -p codel00p-cli --test agent_cli native_provider && cargo test -p codel00p-cli && cargo clippy -p codel00p-cli --all-targets -- -D warnings
```

- [x] **Step 2: Run repo verification**

Run:

```bash
pnpm verify
```

- [x] **Step 3: Commit and push**

Run:

```bash
git add README.md docs/agent-capability-parity.md docs/product-roadmap.md docs/roadmap.md docs/superpowers/plans/2026-06-10-native-agent-provider-modes.md core/crates/codel00p-cli/src/providers.rs core/crates/codel00p-cli/tests/agent_cli.rs
git commit -m "feat: enable native provider agent runs"
git push origin main
```
