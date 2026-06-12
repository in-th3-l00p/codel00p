# Plugin Registry Crate

First slice of [Initiative 1: Plugins & Hooks](../../initiatives/plugins-and-hooks.md),
Phase 1. Introduces the unified extension seam without changing any runtime
behavior.

## Goal

Add a `codel00p-plugin` crate with a `Plugin` trait and `PluginRegistry` that
aggregate tool, lifecycle-hook, event-sink, and provider-profile contributions
behind one narrow waist, so later slices can route built-in and third-party
capability through the same path.

## Scope

- [x] Add `with_tool_arc` to `ToolRegistry` so contributions can be folded in as
      `Arc<dyn Tool>` (additive; last-writer-wins on tool name).
- [x] New `codel00p-plugin` crate depending on `codel00p-harness` and
      `codel00p-providers`.
- [x] `Plugin` trait: `name()` + default-empty `tools`, `lifecycle_hooks`,
      `event_sinks`, `provider_profiles`.
- [x] `PluginRegistry`: `register`, `plugin_names`, `len`/`is_empty`,
      `lifecycle_hooks`, `event_sinks`, `apply_to_tool_registry`,
      `apply_to_provider_registry`, all order-sensitive (later wins).
- [x] Tests: empty registry is a no-op, aggregation works, later plugin
      overrides an earlier plugin's tool (verified by executing the tool) and
      provider profile.
- [x] Add crate to `core/Cargo.toml` workspace members.
- [x] `cargo build` (workspace), `cargo test -p codel00p-plugin`,
      `cargo test -p codel00p-harness`, `cargo fmt --check`, `cargo clippy`.

## Out of scope (later slices of the initiative)

- Wiring `PluginRegistry` into `AgentHarnessBuilder` and the CLI so plugins are
  actually loaded at runtime.
- Migrating built-in tools/providers to register *through* a built-in plugin.
- `[plugins]` config table and `codel00p plugins list/enable/disable`.
- Hook veto folded into the permission policy.
- Out-of-process (WASM) plugins.

## Notes

- Provider contribution is limited to compile-time profiles because
  `ProviderProfile` fields are `&'static`; dynamic providers are a later phase.
- The crate aggregates only — it never executes — so it adds no behavior to the
  agent loop until a later slice consumes it.
