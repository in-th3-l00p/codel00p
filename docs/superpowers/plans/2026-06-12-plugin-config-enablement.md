# Plugin Config-Driven Enablement

Third slice of [Initiative 1: Plugins & Hooks](../../initiatives/plugins-and-hooks.md),
Phase 2. Lets configuration choose which plugins an agent run loads, and ships
the first real built-in plugin so the path is exercised end to end.

## Goal

Add a `[plugins] enabled` config surface and a `codel00p plugins` command, back
them with a `PluginCatalog` allow-list, and have agent runs build their plugin
registry from that config.

## Scope

- [x] `PluginCatalog` in `codel00p-plugin`: id + description + lazy factory,
      `contains`/`ids`/`entries`, and `build(enabled)` returning an
      `UnknownPluginError` for ids with no entry (duplicates ignored).
- [x] `[plugins] enabled` settings: `PluginSettings` on `Settings`, merge,
      `plugins.enabled` as a `StrList` config key (so `config get/set/unset`
      work), and `effective_value` support.
- [x] First built-in plugin: `system-info`, contributing a read-only
      `system_info` tool (host OS/arch + workspace root).
- [x] `codel00p plugins list|enable|disable [--project]`, editing the targeted
      config file's list precisely (via `settings::load_file`).
- [x] `load_plugins(workspace)` builds the registry from layered config; unknown
      enabled ids are skipped with a stderr warning so a stale entry never bricks
      a run.
- [x] Tests: catalog unit tests (build/duplicate/empty/unknown) and a
      `plugins_cli` integration suite (list default-disabled, enable/disable
      round-trip incl. `config get plugins.enabled`, unknown rejected,
      idempotent enable).
- [x] `cargo build`, `cargo test -p codel00p-plugin`, `cargo test -p codel00p-cli`
      (single-threaded — the parallel MCP test is timing-sensitive),
      `cargo fmt --check`, `cargo clippy`.

## Decisions

- **Catalog = allow-list.** Enabling names a built-in by stable id; it is not
  arbitrary code loading. This keeps the governance story intact and is what
  `[plugins] enabled` records.
- **Lenient load, strict enable.** `plugins enable` rejects unknown ids up front;
  `load_plugins` tolerates unknown ids already in config (warns + skips) so
  removing a built-in cannot break existing configs.
- **`system-info` as the reference built-in.** Dependency-free, read-only, no
  conflict with existing tool-sets — a safe, genuinely useful first plugin that
  exercises the whole config -> catalog -> registry -> tool path.

## Out of scope (next slices)

- Wiring `apply_to_provider_registry` into provider-client construction.
- Migrating existing built-in tools/providers to register through bundled
  plugins.
- Multi-event-sink support in the harness.
- Hook veto folded into the permission policy.
- Out-of-process (WASM) plugins and a third-party plugin hub.
