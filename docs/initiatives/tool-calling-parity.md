# Initiative 10: Tool-Calling Parity

Bring codel00p's tool calling up to the level of the best coding agents —
**Claude Code**, **OpenCode**, **Hermes Agent / Hermes function-calling**, and
**OpenHands** — without giving up codel00p's governance (permission scopes,
audit events, stable protocol contracts).

This initiative is the umbrella for the foundational tool-calling work that the
existing [Programmatic Tool Calling (#8)](programmatic-tool-calling.md),
[Sub-Agents (#4)](subagents-delegation.md), and
[Skills (#2)](skills-system.md) initiatives build on. **#8 is the capstone; this
initiative fixes the floor first.**

## Why now

A research pass (2026-06-15) compared codel00p's harness against Hermes
function-calling, Hermes Agent, Claude Code, OpenCode, and OpenHands. codel00p
has a clean `Tool` trait, parallel execution of concurrency-safe tools,
permission scoping, MCP discovery, and a rich event stream — but it is missing
the **floor** every mature agent ships, starting with the model never seeing tool
parameter schemas.

### Reference capabilities (verified, with sources)

- Tool schemas as JSON Schema sent to the model; strict conformance; standard
  `tool_use`/`tool_result` loop — Claude Code / Anthropic tool use
  (platform.claude.com/docs build-with-claude/tool-use), OpenCode Zod→JSON Schema
  (opencode.ai/docs/tools).
- Hermes presents tools as OpenAI-style function objects (`{type:function,
  function:{name,description,parameters}}`) inside `<tools>` and pins an explicit
  output schema; tolerant parsing + schema validation on the way back
  (huggingface.co/NousResearch/Hermes-2-Pro-Mistral-7B,
  github.com/NousResearch/Hermes-Function-Calling).
- Parallel tool calls in one round trip; sub-agent fan-out (Claude Code `Agent`,
  OpenHands `tool_concurrency_limit`, OpenCode subagents).
- Tool-result truncation / persist-to-disk to protect context (Claude Code: Bash
  30k chars → file; MCP 25k tokens; OpenCode line/match caps).
- MCP **Tool Search / progressive disclosure** for large toolsets (~47% token
  reduction reported by Anthropic, code-execution-with-mcp; openclaw
  `tool_search`/`tool_describe` catalogs).
- `tool_choice` control (auto / any / specific / none).
- Programmatic / code-execution tool calling — Hermes `execute_code`, Anthropic
  programmatic tool calling, OpenHands CodeAct — see [#8](programmatic-tool-calling.md).

## Current codel00p state (audit 2026-06-15)

- **Tools never reach the model with a real schema.** `provider_adapter.rs:72-76`
  sends every tool as `ToolDefinition::function(name, "codel00p harness tool:
  {name}", {"type":"object"})` — empty schema, generic description. The
  hand-written `Tool::input_schema()` and `Tool::description()` exist but are
  **discarded**; only `tool_names()` is threaded through. This is the single
  biggest reliability gap.
- **No argument validation.** Bad inputs fail inside `execute()` as
  `InvalidToolInput`; there is no schema validation or self-correction loop.
- **No tool-result truncation.** Large outputs (git diffs, command output, MCP
  results) flow into context unbounded; only `run_command`/`git_diff` take an
  optional byte cap the model must set.
- **No `tool_choice` control** surfaced to providers.
- **MCP tools are callable but not discoverable in-turn**; no tool-search /
  progressive disclosure, so large MCP toolsets bloat the prompt.
- **Sub-agent delegation is a seam, not a built-in.** `delegate_task` exists but
  needs a `SubAgentSpawner` plugged in; no default implementation.
- Parallel execution exists (concurrency-safe batching → `join_all`), permission
  scoping is solid, and MCP discovery + events are good. Those stay.

See the [Agent Capability Parity](../agent-capability-parity.md) doc for the
broader matrix; this initiative tracks the tool-calling slice of it.

## Design principles

- **Governance is non-negotiable.** Every new capability keeps per-call
  permission scopes and audit events. Schema serialization, truncation, and
  validation are transparent to the permission layer.
- **Provider-neutral.** Schema, `tool_choice`, and result shaping go through the
  existing `codel00p-providers` neutral contract and fan out per transport
  (OpenAI / Anthropic / Gemini / Bedrock), so one change covers every provider.
- **Test offline.** Each slice is verifiable with httpmock + unit tests, no live
  credentials.
- **Smallest shippable slices.** Each Scope item below is an independent PR.

## Scope (phased, by leverage)

### Phase 1 — Floor: schema, validation, result hygiene — **SHIPPED 2026-06-15**
- [x] **Serialize real tool schemas + descriptions to providers** — `ToolSpec` +
      `ToolRegistry::specs()` threaded through `HarnessInferenceRequest` into
      `ToolDefinition`, replacing the `{"type":"object"}` stub (PR #13).
- [x] **Validate tool arguments against the schema before dispatch** — a lenient
      JSON Schema subset in `validation.rs`; malformed calls return a structured
      `InvalidToolInput` the model self-corrects from (PR #14).
- [x] **Tool-result truncation with persist-to-disk + preview** —
      `ToolOutputTruncation` (default 16 KiB, builder-configurable) caps the
      model-visible result to a UTF-8-safe head/tail preview, persisting the full
      output to a temp file (PR #15).
- [x] **`tool_choice` control** (auto / required / none / specific) — provider-
      neutral `ToolChoice` serialized across every transport, with an
      `AgentHarness` builder setter (PR #16).

### Phase 2 — Surface & orchestration parity
- [ ] **MCP tool search / progressive disclosure**: advertise only names +
      one-line descriptions for large MCP tool sets, with a `tool_search` /
      `tool_describe` mechanism that loads full schemas on demand.
- [ ] **Default sub-agent spawner** so `delegate_task` works out of the box —
      isolated context window, per-agent tool scoping, parent sees only the final
      result (pairs with [#4](subagents-delegation.md)).
- [ ] **Repo-map / semantic code-search tool** (tree-sitter ranked symbols,
      Aider-style) for navigation beyond `search_text`.
- [ ] **Token-efficient tool results**: pagination/filter params and a
      concise/detailed `response_format` on the heavy tools.

### Phase 3 — Advanced (SOTA)
- [ ] **Programmatic tool calling (`execute_code`)** — see
      [#8](programmatic-tool-calling.md); this initiative's capstone.
- [ ] **JSON Mode / structured output** distinct from tool calling (Hermes
      `<schema>` / Anthropic strict): schema-validated structured responses with a
      recursive retry, for planning and structured edits.
- [ ] **Streaming tool-call arguments** (incremental parse) where the transport
      supports it.

## Dependencies

- [#1 Plugins & Hooks](plugins-and-hooks.md) — tool registration seam.
- [#4 Sub-Agents](subagents-delegation.md) — Phase 2 default spawner.
- [#7 Execution Backends](execution-backends-sandboxing.md) — required before the
  full `execute_code` surface (Phase 3).
- [#8 Programmatic Tool Calling](programmatic-tool-calling.md) — the Phase 3
  capstone.

## Exit criteria

- The model receives every tool with a complete JSON Schema and rich
  description; malformed calls are validated and self-corrected; large results
  never blow the context window; `tool_choice` is controllable; large MCP
  toolsets stay cheap via tool search; sub-agent delegation works out of the box;
  and the model can run a governed multi-tool pipeline in one `execute_code`
  call — all while every underlying call stays permission-scoped and audited.
