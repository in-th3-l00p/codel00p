# GCP Managed Identity Resolver

## Goal

Add a concrete GCP metadata server resolver that can fetch service account
OAuth access tokens while preserving managed-identity route metadata for
bearer-compatible provider routes.

## Scope

- [x] Add failing mocked metadata server tests for token acquisition and
  invalid token responses.
- [x] Add public GCP managed identity resolver APIs for service account access
  tokens.
- [x] Add an async client builder helper that stores the resolved token as
  `CredentialSourceKind::ManagedIdentity`.
- [x] Update provider docs and backlog/roadmap notes.
- [x] Run focused provider tests and full `pnpm verify`.
- [ ] Commit and push without coauthor trailers.
