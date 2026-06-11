# Managed Identity Resolver Boundary

## Goal

Add a provider-side managed identity resolver boundary so future cloud token
acquisition code can inject short-lived credentials while preserving safe route
metadata and enterprise managed-identity policy checks.

## Scope

- [x] Add a failing runtime resolution test for a managed identity resolver.
- [x] Add `ManagedIdentityCredentialRequest` and
  `ManagedIdentityCredentialResolver` public provider APIs.
- [x] Add an `InferenceClientBuilder` method that resolves a credential through
  the resolver and stores it as `CredentialSourceKind::ManagedIdentity`.
- [x] Update provider docs and roadmap/backlog notes.
- [x] Run focused provider tests and full `pnpm verify`.
- [x] Commit and push without coauthor trailers.
