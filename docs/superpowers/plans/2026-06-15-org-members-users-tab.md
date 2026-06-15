# Slice: Real Users tab — `GET /org/members`

Date: 2026-06-15
Roadmap: Milestone 6 (Desktop/Cloud interfaces), TUI initiative Phase 3.

## Goal

Replace the TUI entity browser's "Member listing is pending a backend endpoint"
placeholder with a real, org-scoped Users tab backed by Clerk's membership data.

## Why this slice

It is the smallest TUI Phase-3 item that ships independently and is testable end
to end: a read-only roster needs no token re-mint (unlike writable org switching)
and reuses the existing entity-browser picker plumbing.

## Design

Clerk owns membership; codel00p reads it through the Clerk **Backend API**
(`GET /v1/organizations/{org_id}/memberships`) with a secret key, then projects
it into a protocol type the CLI already knows how to render.

1. **Protocol** (`codel00p-protocol::cloud`): add `OrgMember { user_id, role:
   OrgRole, email: Option, name: Option }` with builder + accessors; export it.
2. **Cloud directory** (`codel00p-cloud::directory`): a `ClerkDirectory { http,
   secret_key, api_base }` with `from_env()` (reads `CLERK_SECRET_KEY`, optional
   `CLERK_API_BASE` override) and `async list_members(org_id) -> Vec<OrgMember>`
   mapping Clerk's `data[].public_user_data` + `role` into `OrgMember`.
3. **State + route**: `AppState` gains an optional `Arc<ClerkDirectory>` set via
   `with_directory`; `main.rs` wires it from env. `GET /org/members` requires an
   active org (any member), 503s when no directory is configured.
4. **Error**: add `ApiError::ServiceUnavailable` (503) for the unconfigured case.
5. **CloudClient** (`cli`): `list_org_members() -> Vec<OrgMember>` over
   `GET /org/members`.
6. **TUI**: `CloudFetch::Users` + `Msg::CloudUsers`; `EntityBrowser.users:
   Picker<OrgMember>` (read-only); fetch on browser open; render the picker in
   the Users tab instead of the placeholder.

## Tests (test-first)

- cloud e2e (`tests/http.rs`, httpmock as the Clerk Backend API): members map
  role + name + email; unconfigured directory → 503; member token still allowed.
- `CloudClient::list_org_members` over a mock (`cloud_client.rs`).
- TUI `update`: `CloudUsers` populates the picker; open fetches Users; Users tab
  navigates without selecting (read-only).

## Out of scope

Writable org switching (Clerk token re-mint), mouse/themes/usage meters,
`list_models`-sourced model picker — tracked separately in the TUI initiative.
