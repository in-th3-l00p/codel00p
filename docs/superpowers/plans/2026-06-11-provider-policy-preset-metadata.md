# Provider Policy Preset Metadata

## Goal

Expose named provider policy preset metadata so cloud and desktop surfaces can
list built-in policy choices and resolve selected IDs into policies.

## Scope

- [x] Add a failing provider policy preset metadata and resolution test.
- [x] Add `ProviderPolicyPreset`, `ProviderPolicy::presets()`, and
  `ProviderPolicy::from_preset(...)`.
- [x] Update provider docs and roadmap/backlog notes.
- [x] Run focused provider tests and full `pnpm verify`.
- [ ] Commit and push without coauthor trailers.
