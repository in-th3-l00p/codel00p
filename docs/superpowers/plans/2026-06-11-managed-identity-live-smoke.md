# Managed Identity Live Smoke Coverage

## Goal

Add default-off live smoke tests and operator guidance for Azure, AWS, and GCP
managed identity credential resolvers.

## Scope

- [x] Add failing support tests for per-cloud managed identity live-test gating.
- [x] Add live ignored tests that verify each resolver returns the expected
  credential kind from the matching cloud metadata runtime.
- [x] Add integration config helpers and safe skip messages with no secret
  values.
- [x] Update provider docs and roadmap/backlog notes.
- [x] Run focused provider tests and full `pnpm verify`.
- [ ] Commit and push without coauthor trailers.
