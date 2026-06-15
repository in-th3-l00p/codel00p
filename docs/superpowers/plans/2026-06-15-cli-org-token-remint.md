# Slice: CLI org token re-mint

Date: 2026-06-15
Roadmap: TUI initiative Phase 3, writable org switching prerequisite.

## Goal

Let the CLI request a fresh Clerk `cli` JWT scoped to a specific organization,
so the TUI Org tab can switch organizations by reusing the same browser loopback
login flow.

## Design

1. `codel00p login --org <org_id>` appends `org_id` to the `/connect/cli`
   handoff URL.
2. `/connect/cli` validates the normal `port` + `state` callback parameters.
3. When `org_id` is present and is not the active Clerk org, the page renders a
   client-side activation handoff that calls Clerk `setActive({ organization })`
   and reloads the same URL with `activated=1`.
4. The server page then calls `getToken({ template: "cli" })`; the minted token
   carries the active organization claims from Clerk.
5. Existing CLI credential storage continues to decode and persist `org_id`,
   `org_name`, and `email` from the token claims.

## Tests

- CLI login URL builder appends `org_id` only when requested.
- Existing callback parsing and claim decoding remain unchanged.
- Landing app typecheck/build covers the new client handoff component.

## Out of scope

The TUI picker/action that invokes this flow from the Org tab. That becomes a
small follow-up once the re-mint primitive is available.
