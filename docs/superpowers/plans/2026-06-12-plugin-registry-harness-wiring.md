# Plugin Registry Harness Wiring

Second slice of [Initiative 1: Plugins & Hooks](../../initiatives/plugins-and-hooks.md),
Phase 1. Makes the registry from the previous slice actually reachable at
runtime, without changing default behavior.

## Goal

Let a `PluginRegistry` contribute its tools and lifecycle hooks to a running
agent, and route the CLI's harness assembly through that seam, so a later slice
only has to populate the registry from config.

## Scope

- [x] `AgentHarnessBuilder::lifecycle_hook_arc` so type-erased hooks can be
      folded in (mirrors the earlier `ToolRegistry::with_tool_arc`).
- [x] `PluginRegistry::apply_to_harness_builder` folding plugin lifecycle hooks
      onto a builder (re-exports `AgentHarnessBuilder`).
- [x] Integration test (`tests/harness_wiring.rs`) running a full turn with a
      scripted model, proving a plugin-contributed tool executes and a
      plugin-contributed lifecycle hook fires.
- [x] CLI: `load_plugins()` seam + route `build_agent_harness` tools through
      `apply_to_tool_registry` and the builder through `apply_to_harness_builder`.
- [x] `cargo build`, `cargo test -p codel00p-plugin`, `cargo test -p codel00p-cli`
      (95 tests, behavior-neutral with the empty registry),
      `cargo test -p codel00p-harness`, `cargo fmt --check`, `cargo clippy`.

## Decisions

- **Wiring lives in `codel00p-plugin`, not the harness.** `codel00p-plugin`
  depends on `codel00p-harness`, so the harness cannot depend back on it; the
  `apply_to_*` convenience methods take the builder/registries as parameters and
  call their public methods.
- **Event sinks are not folded yet.** The harness drives a single event sink and
  the CLI already owns it for stdout streaming. `apply_to_harness_builder`
  therefore folds lifecycle hooks only; `PluginRegistry::event_sinks()` is
  exposed for callers, and multi-sink support is deferred.
- **Provider profiles are not folded into the harness builder.** Provider
  selection happens before the harness is built (in provider-client
  construction), so plugin providers will be wired into that path in a later
  slice via `apply_to_provider_registry`.
- **`load_plugins()` returns an empty registry for now.** Establishing the call
  site keeps default tool/hook behavior identical while giving the config-driven
  enablement slice one place to register plugins.

## Out of scope (next slices)

- `[plugins]` config table and `codel00p plugins list/enable/disable`.
- Wiring `apply_to_provider_registry` into provider-client construction.
- Migrating built-in tools/providers to register through a built-in plugin.
- Multi-event-sink support in the harness.
- Hook veto folded into the permission policy.
