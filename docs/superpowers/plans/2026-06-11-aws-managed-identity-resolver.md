# AWS Managed Identity Resolver

## Goal

Add a concrete AWS IMDSv2 instance profile resolver that can fetch temporary
SigV4 credentials for Bedrock while preserving managed-identity route metadata.

## Scope

- [x] Add failing mocked IMDSv2 tests for token acquisition, role lookup, and
  invalid credential responses.
- [x] Add public AWS managed identity resolver APIs for instance profile role
  credentials.
- [x] Add an async client builder helper that stores the resolved credentials as
  `CredentialSourceKind::ManagedIdentity`.
- [x] Update provider docs and backlog/roadmap notes.
- [x] Run focused provider tests and full `pnpm verify`.
- [ ] Commit and push without coauthor trailers.
