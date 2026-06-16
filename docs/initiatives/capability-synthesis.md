# Initiative 11: Capability Synthesis

Make codel00p the only agent whose **toolset compounds**: every solved task can
be frozen into a named, parameterized, reviewed, permission-scoped tool, so the
agent — and the whole org's fleet — gets measurably more capable and cheaper to
run (fewer round-trips) the more it is used.

## Why this is uniquely possible here

It requires four primitives that no other agent has together, and codel00p
already ships each:

- **Programmatic pipelines** (`pipeline.rs`) — `run_pipeline` is governed,
  multi-step, with `{{...}}` data-passing. A capability is a *frozen* one.
- **Per-call governance** (`permissions.rs`) — every step is permission-scoped
  and audited, so a synthesized capability can declare its *minimal* scope (the
  max of its steps) and stay gated step-by-step.
- **Reviewed memory / proposal queues** (`memory.rs`, `learning.rs`) — capability
  candidates go through review/approval rather than being trusted blindly.
- **Progressive disclosure** (`tool_registry.rs`) — an ever-growing capability
  library stays cheap in the prompt via `tool_search` / `tool_describe`.

The result is a flywheel competitors can't copy without rebuilding the
governance substrate: task → verified capability → cheaper next task → more
capabilities, where governance scales *with* capability because each capability
carries its own audited scope.

## Shipped — Slice 1 (2026-06-16)

The freeze-pipeline-into-reviewed-capability core, in `capability.rs`:

- `Capability { name, description, parameters, steps }` — a serializable, frozen
  parameterized pipeline. `parameters` is the tool's JSON Schema; step inputs
  reference `{{params.<name>}}` (call args) and `{{steps.N.field}}` (earlier
  outputs).
- `CapabilityTool` — exposes a capability as a first-class `Tool`. On call it
  seeds `params` and runs the frozen steps through the shared `PipelineEngine`
  (extracted from `run_pipeline`), so **every step is dispatched through the
  harness's `ToolRegistry` + `PermissionPolicy`** — a denied step never runs.
  Its `permission_scope` is the max of its steps.
- `propose_capability` + `CapabilityProposalSink` — the agent submits a
  candidate to a review queue (`FileCapabilityProposalSink` writes
  `<dir>/<name>.json`); it is validated (legal name, object param schema,
  well-formed steps) and the inferred scope reported, but **not** executed or
  registered until approved.
- `load_capabilities(dir)` + `capability_tools(...)` + builder `.capabilities()`
  / `.capability_proposals()` — load approved capabilities and register them as
  tools (advertised, or deferred behind progressive disclosure for large
  libraries).

Verified end-to-end, including a **live test against OpenRouter** in which a real
model discovers a synthesized `scaffold_module` capability and accomplishes a
two-file scaffold in a single governed tool call
(`tests/capability_turn.rs`, gated on `CODEL00P_PROVIDER_OPENROUTER_API_KEY`).

## Shipped — Slice 2 (2026-06-16): auto-extraction + verification

The loop now closes itself, and promotion is gated on a working replay.

- **Auto-extraction** (`CapabilityExtractor`): after each completed turn the
  harness runs an extractor over the executed tool calls and auto-proposes a
  capability to the same review sink — no explicit `propose_capability` needed.
  Wired via builder `.capability_extractor()` + `.capability_proposals()`, fired
  from the turn alongside skill extraction. (`ExecutedToolCall` now also carries
  the tool `input`, so extractors and audits see what was run.)
  - `PipelineCapabilityExtractor` — deterministic, no extra inference: freezes a
    fully-successful multi-step `run_pipeline` verbatim into a candidate named
    after the goal.
  - `ModelCapabilityExtractor` — LLM-assisted "tools that write tools": asks the
    model to *generalize* a concrete successful pipeline into a parameterized
    capability (lifting literals into `{{params.*}}`), returning JSON it parses
    and validates. Proven live: a real model turned a hard-coded `widget`
    scaffold into a `{{params}}`-driven capability that then passed verification.
- **Verification gate** (`verify_capability`): replays a capability once on a
  fresh temp workspace with sample params and requires every step to complete —
  so a promoted capability is *trusted to run*, not merely well-formed. A denied
  or erroring step fails verification.

## Next slices

- **Org propagation**: share approved capabilities across the fleet via reviewed
  team memory, with usage tracking and a curator that retires stale ones.
- **Composition**: let capabilities call other approved capabilities (bounded
  depth) so higher-order capabilities emerge.
- **Richer verification**: seed fixtures + run a build/test command (pairs with
  transactional turns / execution backends #7) for capabilities that mutate
  existing files.
