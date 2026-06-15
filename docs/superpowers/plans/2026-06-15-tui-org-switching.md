# Slice: TUI organization switching

Date: 2026-06-15
Roadmap: TUI initiative Phase 3, writable org switching.

## Goal

Let the TUI Org tab switch the active Clerk organization by selecting from the
caller's real organization memberships, then reusing the CLI browser login flow
to mint a fresh org-scoped token.

## Design

1. Add `GET /orgs` to the cloud service, backed by Clerk user organization
   memberships.
2. Expose the route through the TypeScript SDK and Rust `CloudClient`.
3. Fetch organizations when the TUI entity browser opens.
4. Render the Org tab as a picker while still showing the current active org and
   role.
5. On selection, temporarily leave the terminal alternate screen, run
   `codel00p auth login --org <org_id>`, restore the TUI, and refresh cloud
   viewer/projects/users/orgs.

## Tests

- Cloud route maps Clerk user memberships into protocol `OrgRef` values.
- Cloud route reports 503 when the Clerk directory is unavailable.
- TUI update tests cover org fetch population and switch effect emission.
- Full workspace verify covers TypeScript SDK exports, Next build, Rust tests,
  sqlite feature tests, and clippy.

## Out of scope

Live Clerk browser validation requires real Clerk credentials and an interactive
browser session. The implementation is structured so live validation exercises
the same `auth login --org` primitive already used by the CLI.
