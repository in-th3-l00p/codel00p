# Plugin Provider Wiring

Fourth slice of [Initiative 1: Plugins & Hooks](../../initiatives/plugins-and-hooks.md),
Phase 2. Lets plugins contribute inference providers, not just tools and hooks.

## Goal

Fold plugin-contributed provider profiles into the registry the agent's
inference client is built from, so an enabled plugin can add a provider that
routes exactly like a built-in one.

## Scope

- [x] `providers::build_provider_client_with(registry, provider, preset)` takes a
      caller-supplied `ProviderRegistry`; the credential-missing hint now reads
      env vars from that registry (so plugin providers get a correct hint).
- [x] `build_agent_harness` loads plugins once and reuses the registry for
      providers, tools, and hooks: the provider client is built from
      `plugins.apply_to_provider_registry(default_registry())`.
- [x] Removed the now-unused `build_provider_client` / `provider_env_vars`
      wrappers (tests fold a registry directly).
- [x] Test: a plugin contributing a `ProviderProfile` is routable end to end —
      folded into the built-in registry, built via `build_provider_client_with`,
      and resolved with the expected env credential source.
- [x] `cargo build`, `cargo test -p codel00p-cli` (single-threaded — the
      parallel MCP test is timing-sensitive), `cargo fmt --check`,
      `cargo clippy`.

## Decisions

- **Plugins are loaded once per run.** The same `PluginRegistry` now feeds three
  surfaces (providers, tools, hooks), so load order moved above provider-client
  construction.
- **Compile-time providers only.** `ProviderProfile` fields are `&'static`, so
  this covers in-process providers; dynamically configured providers remain a
  later phase.
- **No built-in provider plugin yet.** Like the earlier tool/hook wiring, the
  seam is proven by a test plugin; a shipped provider plugin can come later.

## Out of scope (next slices)

- Surfacing plugin providers in `codel00p providers list/show` (those run before
  per-workspace plugin config is resolved).
- Migrating built-in providers to register through a bundled plugin.
- Deferred toolset mutation; multi-event-sink support; hook veto.
- Out-of-process (WASM) plugins.
