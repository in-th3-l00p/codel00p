# Slice: Serialize real tool schemas + descriptions to providers

Date: 2026-06-15
Initiative: [Tool-Calling Parity (#10)](../../initiatives/tool-calling-parity.md),
Phase 1. Backlog priority #1.

## Goal

Send each tool to the model with its **real JSON Schema and description** instead
of the current stub. This is the single highest-leverage tool-calling fix.

## The bug

`provider_adapter.rs:72-76` builds every tool definition as:

```rust
for tool_name in request.tool_names() {
    builder = builder.tool(ToolDefinition::function(
        tool_name,
        format!("codel00p harness tool: {tool_name}"),
        json!({ "type": "object" }), // ← empty schema
    ));
}
```

The model receives only the tool *name* plus a generic description and an empty
parameter object. The hand-written `Tool::input_schema()` and
`Tool::description()` are discarded — `HarnessInferenceRequest` only carries
`tool_names: Vec<String>` (`turn.rs:42`). Every transport
(OpenAI/Anthropic/Gemini/Bedrock) already forwards `ToolDefinition.parameters` as
JSON Schema, so the fix is purely about *threading the real data through*.

## Design

1. **`ToolSpec`** (harness): `{ name: String, description: String, input_schema:
   serde_json::Value }`. Add `ToolRegistry::specs(&self) -> Vec<ToolSpec>` that
   reads `name()`/`description()`/`input_schema()` off each registered
   `Arc<dyn Tool>` (registry is a `BTreeMap<String, Arc<dyn Tool>>`).
2. **`HarnessInferenceRequest`**: replace `tool_names: Vec<String>` with `tools:
   Vec<ToolSpec>` (keep a `tool_names()` accessor that maps `tools` → names for
   existing callers/events). Rename `with_tool_names` → `with_tools`.
3. **`agent/turn.rs:86`**: pass `self.tools.specs()` instead of the name list
   when building the request (the agent already owns the `ToolRegistry`).
4. **`provider_adapter.rs`**: build `ToolDefinition::function(spec.name,
   spec.description, spec.input_schema)` from the specs — delete the stub.

MCP tools already expose `input_schema()` via the `McpTool` wrapper, so they get
real schemas through the same path for free.

## Tests (offline, test-first)

- `tool_registry`: `specs()` returns name/description/schema for the default
  tool sets; a known tool (e.g. `read_file`) has its `path` property in the
  schema.
- `provider_adapter`: `build_provider_request` emits `ToolDefinition`s whose
  `parameters` equal the tools' real `input_schema()` (not `{"type":"object"}`)
  and whose `description` is the tool's real description.
- A harness turn test (existing `ModelClient` mock) asserts the request handed to
  the model carries the populated tool definitions.

## Out of scope (later Phase 1 slices)

Argument validation against the schema, result truncation, and `tool_choice` —
separate slices in the initiative.

## Exit criteria

The model receives every built-in and MCP tool with its complete JSON Schema and
real description; no `{"type":"object"}` stub remains; all transports carry it.
