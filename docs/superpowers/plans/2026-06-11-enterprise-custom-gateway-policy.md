# Enterprise Custom Gateway Policy

## Goal

Add an enterprise policy template for organizations that route inference through
a configured OpenAI-compatible gateway profile instead of direct provider IDs.

## Scope

- [x] Add a failing provider policy test for custom gateway routes.
- [x] Add `ProviderPolicy::enterprise_custom_gateway()`.
- [x] Update provider docs and roadmap/backlog notes.
- [x] Run focused provider tests and full `pnpm verify`.
- [ ] Commit and push without coauthor trailers.
