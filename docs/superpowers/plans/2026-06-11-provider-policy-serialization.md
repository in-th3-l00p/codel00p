# Provider Policy Serialization

## Goal

Make provider policies persistable as safe JSON control-plane defaults for
cloud and desktop surfaces.

## Scope

- [x] Add a failing provider policy serialization round-trip test.
- [x] Add serde support for `ProviderPolicy` with empty/default fields omitted.
- [x] Update provider docs and roadmap/backlog notes.
- [x] Run focused provider tests and full `pnpm verify`.
- [ ] Commit and push without coauthor trailers.
