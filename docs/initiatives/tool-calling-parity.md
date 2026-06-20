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
- [x] **MCP tool search / progressive disclosure** (shipped 2026-06-16):
      `ToolRegistry` now supports *deferred* tools — registered and executable
      but withheld from the prompt. `advertised_specs()` (used by the turn)
      returns the core tools plus synthetic `tool_search` (ranked keyword search
      over hidden tools) and `tool_describe` (loads a hidden tool's full schema),
      which the registry answers from its own catalog. A deferred tool stays
      callable by name once discovered. The CLI folds the combined MCP tool set
      in as deferred past a 15-tool threshold; small setups are unchanged.
      Mirrors Anthropic's code-execution-with-MCP disclosure.
- [x] **Default sub-agent spawner** (shipped 2026-06-16): `HarnessSubAgentSpawner`
      runs each `delegate_task` as an isolated child `AgentHarness` turn — fresh
      session, the child's own (leaf) tool set, its own permission ceiling; the
      parent sees only the child's final summary (pairs with
      [#4](subagents-delegation.md)).
- [x] **File-navigation tools** (shipped 2026-06-16): `find_files` (glob: `*`,
      `**`, `?`; ignore-aware) and `grep` (regex content search with `glob`
      filter, case-insensitivity, `context_lines`, and `offset`/`limit` paging;
      ignore-aware). These give the model the find-by-name and search-by-regex
      capabilities every mature coding agent ships, beyond the substring
      `search_text`.
- [x] **Repo-map / semantic code-search tool** (shipped 2026-06-16): `repo_map`
      extracts code symbols (functions, types, classes) across the workspace
      with a dependency-light multi-language heuristic (Rust, Python, JS/TS, Go,
      Java, Ruby, C/C++) and ranks them Aider-style by cross-repo reference
      frequency (a cheap PageRank proxy), bounded by `max_files` /
      `max_symbols_per_file` and scopable by `path` / `glob`. (A tree-sitter
      backend for exact parsing remains a possible follow-up.)
- [x] **Background command execution** (shipped 2026-06-16): `run_command`
      gains `background: true`, returning a `process_id` for a long-running
      process instead of blocking, plus `process_output` (incremental
      stdout/stderr since last read), `process_list`, and `process_kill`. Output
      is drained continuously into byte-capped buffers; the four command tools
      share one process store. The background-shell capability mature agents use
      for dev servers / watchers.
- [x] **Working-plan tool** (shipped 2026-06-16): `update_plan` keeps a
      structured to-do list (steps with `pending` / `in_progress` / `completed`,
      one in-progress at a time) in a `PlanStore` a UI can read — the planning
      aid of Claude Code's `TodoWrite` / Codex's `update_plan`. Always advertised
      by the CLI agent.
- [x] **Token-efficient tool results** (shipped 2026-06-20): a concise/detailed
      `verbosity` param on the heavy tools (`repo_map` names-only, `git_diff`
      `--stat`, `git_log` oneline, `grep` match-only, `list_files` paths) — default
      `detailed` keeps current output — plus real `offset`/`limit` pagination on
      `list_files` (others already had paging/caps).

### Phase 3 — Advanced (SOTA)
- [x] **Programmatic tool calling (`run_pipeline`)** (shipped 2026-06-16): the
      governed, no-sandbox form of [#8](programmatic-tool-calling.md). The model
      declares an ordered list of `{ tool, input, id? }` steps and the harness
      runs them in one inference, forwarding earlier outputs into later steps via
      `{{steps.N.field}}` / `{{id.field}}` templates. **Every step is dispatched
      through the harness's own `ToolRegistry` and `PermissionPolicy`**, so a
      denied step does not run and authority never moves into the pipeline — only
      orchestration does. Opt in with the builder's `programmatic_tooling(true)`
      or the CLI `pipeline` tool set. Arbitrary in-process *code* execution
      (Hermes `execute_code`) still waits on an isolating execution backend
      ([#7](execution-backends-sandboxing.md)).
- [x] **JSON Mode / structured output** (shipped 2026-06-16): a provider-neutral
      `ResponseFormat` (text / json_object / json_schema) on the request +
      `AgentHarness` builder, serialized where natively supported — OpenAI-style
      chat (incl. Azure/GitHub) `response_format` and Gemini
      `generationConfig.responseMimeType`/`responseSchema`; omitted for Anthropic
      / Bedrock / the Responses API. (Schema-validated recursive retry remains a
      follow-up.)
- [x] **Capability synthesis — Slice 1** (shipped 2026-06-16): freeze a governed
      `run_pipeline` into a named, parameterized, reviewed tool. `Capability` +
      `CapabilityTool` run the frozen steps through the shared `PipelineEngine`
      (so every step stays permission-gated), `propose_capability` queues
      candidates to a review sink, and approved capabilities register as
      first-class tools. Proven end-to-end against a live OpenRouter model. The
      toolset now *compounds*. See [#11](capability-synthesis.md).
- [x] **Capability synthesis — Slice 2** (shipped 2026-06-16): the loop closes
      itself. A `CapabilityExtractor` auto-proposes a capability after each turn
      (deterministic `PipelineCapabilityExtractor`, or LLM-assisted
      `ModelCapabilityExtractor` that *generalizes* a pipeline into a
      parameterized capability), and `verify_capability` gates promotion by
      replaying the capability on a fresh workspace. Both the auto-extraction and
      the LLM generalization+verification are proven against a live model.
- [x] **Streaming tool-call arguments** (shipped 2026-06-20): incremental
      tool-call argument deltas are surfaced through the streaming sink
      (`TokenSink::on_tool_call_delta`) and a new `ToolCallArgsDelta` event, wired
      across the Chat Completions / Azure / Anthropic Messages / Bedrock Converse /
      Gemini transports (the Responses transport only exposes tool calls at the
      terminal body, so it is documented as unsupported). Final assembled tool
      calls and non-streaming behavior are unchanged.

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
