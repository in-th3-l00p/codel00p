# TypeScript Provider Policy Presets

## Goal

Expose the built-in provider policy preset metadata to TypeScript cloud,
desktop, and SDK surfaces without making each app copy the Rust preset IDs.

## Scope

- [x] Add a failing protocol contract check for the built-in preset metadata.
- [x] Add `providerPolicyPresets`, preset ID types, and lookup helpers to
  `@codel00p/protocol-ts`.
- [x] Re-export preset metadata and helper accessors from `@codel00p/sdk`.
- [x] Update protocol, SDK, provider, roadmap, and backlog docs.
- [x] Run focused protocol/SDK checks and full repository verification.
- [ ] Commit and push without coauthor trailers.
