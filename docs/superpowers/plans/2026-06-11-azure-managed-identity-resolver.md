# Azure Managed Identity Resolver

## Goal

Add a concrete Azure managed identity token resolver that can fetch short-lived
provider credentials from IMDS while preserving existing managed-identity route
metadata.

## Scope

- [x] Add failing mocked IMDS tests for token acquisition and invalid responses.
- [x] Add public Azure managed identity resolver APIs with typed identity
  selectors.
- [x] Add an async client builder helper that stores the resolved token as an
  existing `CredentialSourceKind::ManagedIdentity` route credential.
- [x] Update provider docs and backlog/roadmap notes.
- [x] Run focused provider tests and full `pnpm verify`.
- [ ] Commit and push without coauthor trailers.
