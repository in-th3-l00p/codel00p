# Initiative 1: Plugins & Hooks System

## Goal

Give codel00p an extension boundary so tools, providers, memory backends, and
lifecycle behavior can be added without modifying core crates — the substrate
every other initiative in this directory builds on.

## Why (Hermes reference)

Hermes treats plugins as the default extension path and reserves core tools for
last resort ("the footprint ladder": extend existing code -> CLI + skill ->
service-gated tool -> plugin -> MCP server -> core tool). It ships:

- **General plugins** registering pre/post tool and LLM hooks, new tools, and
  CLI subcommands, discovered from `~/.hermes/plugins/`, `./.hermes/plugins/`,
  and pip entry points.
- **Model-provider plugins** — every inference backend is a plugin registering a
  `ProviderProfile`; user plugins override bundled ones (last-writer-wins).
- **Specialized plugin directories** for memory, context engine, and image
  generation, all following an ABC + orchestrator pattern.

## Current codel00p state

- Providers live in `codel00p-providers` and are compiled in; adding one is a
  core-crate change.
- Tools (`read_file`, `apply_patch`, `run_command`, `git_*`, MCP-routed) live in
  `codel00p-harness` behind `PermissionScope`.
- The harness already exposes `AgentEventSink` and `TokenSink` trait seams, and
  memory lifecycle hooks (`ProjectMemoryProvider`, `MemoryCandidateSink`,
  `ExplicitTurnMemoryExtractor`). These are the closest thing to a hook system
  today, but they are wired in Rust, not discovered at runtime.
- MCP is the only runtime tool-extension path that exists.

## Design

Two extension surfaces, because Rust cannot load arbitrary plugin code the way
Python can:

### A. In-process trait registries (compile-time plugins)

Formalize the existing seams into a single `codel00p-plugin` crate that defines
registry traits and a `PluginRegistry` assembled at startup:

- `ToolPlugin` — contributes one or more tools with declared `PermissionScope`.
- `ProviderPlugin` — registers a `ProviderProfile` (mirrors Hermes
  last-writer-wins override semantics; user registration wins over built-in).
- `MemoryProviderPlugin` — alternative `MemoryRepository` backends.
- `LifecycleHook` — `pre_tool`, `post_tool`, `pre_inference`, `post_inference`,
  `turn_start`, `turn_end` callbacks, each receiving typed protocol events and
  able to observe, annotate, or veto (for `pre_tool`, fold into the existing
  permission decision rather than a second veto path).

Built-in providers and tools migrate to register through this registry so the
core and the extension path use one code path.

### B. Out-of-process plugins (runtime extension)

For third parties who cannot recompile codel00p:

- **MCP is the primary runtime tool-extension path** and already exists — lean
  on it. The footprint ladder for codel00p becomes: extend existing tool ->
  CLI + skill ([#2](skills-system.md)) -> MCP server -> in-process plugin ->
  core tool.
- **WASM component plugins** (later phase) for hooks/tools that need to run
  in-process but be distributed as artifacts, sandboxed via the component model.

### Hook discovery and config

- Declare enabled plugins in layered TOML (`[plugins]` table in
  `~/.codel00p/config.toml` and `./.codel00p/config.toml`), consistent with the
  existing config system.
- Hooks must respect prompt-cache stability: a hook that mutates the system
  prompt or toolset mid-conversation invalidates provider prompt caching, so
  toolset-mutating hooks default to next-turn application (mirror Hermes's
  deferred-invalidation rule).

### Governance fit

- Every plugin-contributed tool flows through the existing `PermissionScope` /
  `PermissionMode` system and emits the same `AgentEvent`s — no plugin gets an
  unaudited path to the workspace or shell.
- Cloud control plane (`codel00p-cloud`) can later gate which plugins an org
  allows, reusing provider-policy-preset patterns.

## Scope

### Phase 1 — Internal registry (no new capability, pure refactor)
- [x] Add `codel00p-plugin` crate with a `Plugin` trait + `PluginRegistry` that
      aggregates tools, lifecycle hooks, event sinks, and provider profiles
      (last-writer-wins). Slice:
      [2026-06-12-plugin-registry-crate](../superpowers/plans/2026-06-12-plugin-registry-crate.md).
- [x] Wire `PluginRegistry` into `AgentHarnessBuilder` + CLI so plugins load at
      runtime (tools + lifecycle hooks; event sinks/providers deferred). Slice:
      [2026-06-12-plugin-registry-harness-wiring](../superpowers/plans/2026-06-12-plugin-registry-harness-wiring.md).
- [ ] Migrate built-in tools and providers to register through the registry.
- [ ] Route existing `AgentEventSink`/memory hooks through `LifecycleHook`.
- [ ] Golden tests proving identical behavior pre/post refactor.

> Note: the single unified `Plugin` trait subsumes the originally sketched
> `ToolPlugin`/`ProviderPlugin`/`MemoryProviderPlugin` split — a plugin
> contributes any subset of surfaces via default-empty methods, which is simpler
> and matches how a real plugin spans several surfaces. A `MemoryRepository`
> contribution surface remains future work.

### Phase 2 — Config-driven enablement
- [ ] `[plugins]` config table (layered, project + user).
- [ ] `codel00p plugins list` / `enable` / `disable` CLI surface.
- [ ] Deferred toolset mutation to preserve prompt caching.

### Phase 3 — Out-of-process plugins
- [ ] Document MCP as the runtime tool-extension path; align the footprint
      ladder docs.
- [ ] Prototype WASM component plugin host for in-process third-party hooks.
- [ ] Cloud org-level plugin allowlist.

## Risks & open questions

- **Rust vs Python**: Hermes's plugin ergonomics rely on dynamic import; we
  cannot match that exactly. The honest answer is "MCP + in-process trait
  registry + (later) WASM," not "drop a `.rs` file in a folder."
- **Hook veto semantics**: avoid two competing veto paths; fold `pre_tool` into
  the permission policy.
- **Cache invalidation**: any hook touching the system prompt is a performance
  footgun; lint for it.

## Dependencies

- None upstream. This is the substrate; [#2](skills-system.md),
  [#5](scheduling-cron.md), [#6](messaging-gateway.md), and
  [#7](execution-backends-sandboxing.md) all become cleaner once it exists.

## Exit criteria

- A new tool or provider can be added by registering a plugin, with no edits to
  `codel00p-harness` or `codel00p-providers` internals, and it inherits
  permission scoping, audit events, and config layering automatically.
