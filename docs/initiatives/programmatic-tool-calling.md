# Initiative 8: Programmatic Tool Calling (`execute_code`)

> **Status (2026-06-16):** the governed, no-sandbox half shipped as the
> `run_pipeline` tool — see [Tool-Calling Parity Phase 3](tool-calling-parity.md).
> The model declares an ordered list of `{ tool, input, id? }` steps with
> `{{...}}` data-passing, and the harness runs them in one inference; **each step
> goes through the harness's own `ToolRegistry` + `PermissionPolicy`**, so a
> denied step never runs. What remains below is the *arbitrary code* form
> (`execute_code`), which still depends on an isolating execution backend
> ([#7](execution-backends-sandboxing.md)).

## Goal

Let the model collapse a multi-step tool pipeline into a single executed code
block, instead of paying one model round-trip per tool call.

## Why (Hermes reference)

Hermes ships **Programmatic Tool Calling via `execute_code`**, which "collapses
multi-step pipelines into single inference calls." Rather than call tool A, wait,
call tool B, wait, the model writes a short program that orchestrates several
tools/operations and runs it once — fewer round-trips, lower latency, lower
token cost on long pipelines.

## Current codel00p state

- The harness runs a classic loop: model emits one (or a concurrency-safe batch
  of) tool call(s) per iteration, results return, repeat until no more calls or
  `--max-iterations` is hit.
- Read-only tools are already marked concurrency-safe and can batch — a partial
  step toward this, but still one model turn per batch.
- No mechanism for the model to express "run this sequence of operations with
  local control flow" in one shot.

## Design

Add an `execute_code` tool (registered via [#1](plugins-and-hooks.md)) that runs
a short script in a constrained runtime with **the codel00p tools exposed as
callable functions**, executed through the active
[terminal backend](execution-backends-sandboxing.md).

### Runtime
- A sandboxed scripting environment (start with one language — e.g. a JS/Python
  runtime) where codel00p tools are bound as functions
  (`read_file`, `search_text`, `run_command`, etc.).
- Every in-script tool call still passes through the **same permission scope and
  audit events** — the script cannot escape the permission system; it is sugar
  over the existing tool dispatch, not a bypass.
- Runs through the active terminal backend, so isolation
  ([#7](execution-backends-sandboxing.md)) applies.

### Why it stays governed
- This is the key risk: `execute_code` could become an unaudited shell. The
  design constraint is that the script's tool calls are individually scoped and
  logged exactly like direct tool calls; only *orchestration* moves into the
  script, not *authority*.
- Output (stdout + structured results) returns to the model as a single tool
  result.

### When the model uses it
- The harness advertises `execute_code` for pipelines; the model chooses it for
  multi-step deterministic work (e.g. "for each failing test, read the file,
  locate the assertion, summarize") and uses normal tool calls otherwise.

## Scope

### Phase 1 — Sandboxed runtime — **SHIPPED 2026-06-20**
- [x] `execute_code` tool with a constrained scripting runtime (Rhai, sandboxed —
      no fs/net/process access except the bound tools), resource-limited
      (operations/recursion/string/array caps + a wall-clock deadline). In-script
      `run_command` dispatches through the active terminal backend, so it inherits
      local/Docker/SSH isolation.
- [x] Bind a minimal set of read-only tools as in-script functions
      (`read_file`/`list_files`/`search_text`/`find_files`/`grep`/`repo_map`),
      plus a generic `call_tool(name, #{...})`.
- [x] Per-call permission scoping + audit events identical to direct calls —
      every in-script call goes through the shared `dispatch_tool` (scope →
      `PermissionRequest` → policy decision → execute) and emits the same
      `ToolCall*`/`Permission*` events a direct call does.

### Phase 2 — Full tool surface — **SHIPPED 2026-06-20**
- [x] Expose write/command/git tools in-script under their scopes
      (`create_file`/`update_file`/`delete_file`/`apply_patch`/`run_command`/
      `git_*`), each governed per-call.
- [x] Structured result capture (the script return value as JSON + a per-call
      summary); size/time limits (result cap, op/wall-clock bounds).

### Phase 3 — Guidance — **SHIPPED 2026-06-20**
- [x] Prompt/affordance tuning so the model picks `execute_code` for dynamic
      logic and direct calls / `run_pipeline` otherwise: the `execute_code`
      description steers it toward dynamic control flow, and `run_pipeline`'s
      description now cross-references `execute_code` for non-fixed chains.
      (Measuring round-trip/token savings remains a follow-up.)

## Risks & open questions

- **Audit bypass**: the whole value of codel00p's governance depends on
  in-script calls being individually scoped/logged — non-negotiable.
- **Sandboxing**: arbitrary code execution must run inside an isolating backend
  ([#7](execution-backends-sandboxing.md)); do not ship the full tool surface
  before Docker isolation exists.
- **Language choice**: one runtime first; resist breadth.
- **Determinism/replay**: a script's interleaved tool calls must still replay
  coherently in the session log.

## Dependencies

- [#1 Plugins & Hooks](plugins-and-hooks.md) (tool registration).
- [#7 Execution Backends](execution-backends-sandboxing.md) (isolation for
  arbitrary code) — Phase 2+ should not precede Docker isolation.

## Exit criteria

- The model can execute a multi-step, multi-tool pipeline in one `execute_code`
  call with measurable round-trip/token savings, while every underlying tool
  call remains individually permission-scoped and audited.
