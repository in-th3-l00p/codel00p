# Agentic Backlog

This backlog is the working queue for the agentic loop. It is ordered by current
leverage, not by final product importance.

For the larger Hermes-parity capabilities not yet on this queue (plugins and
hooks, skills, self-improvement loop, sub-agents, scheduling, messaging gateway,
execution backends, programmatic tool calling, TUI), see the
[Initiatives](initiatives/README.md) epics. As each initiative starts, break its
next phase into dated slice plans under `superpowers/plans/` and add the active
slice here.

## Session marker (2026-06-15)

Latest release: **v0.2.0**. This session shipped the **real TUI Users tab** —
`GET /org/members` (Clerk Backend API → `OrgMember` protocol type →
`CloudClient::list_org_members` → entity-browser picker), replacing the
"pending a backend endpoint" placeholder. Landed on `main` (see plan
`docs/superpowers/plans/2026-06-15-org-members-users-tab.md`); verified green
end to end (`cargo fmt --check`, `cargo test --workspace`, `cargo clippy
--workspace --all-targets -D warnings`) after installing the Rust toolchain in
this shell. Suggested next slice: the
remaining cloud-backed TUI gap — **Clerk token re-mint** in `login.rs` for
writable org switching from the Org tab — then resume the standing priorities
below (provider route intelligence / memory).

## Active Priority

### 1. Provider Route Intelligence

Goal: make provider routing policy-aware, auditable, and ready for team control.

Build:

- model capability metadata; started with raw catalog labels plus typed tools,
  streaming, vision, and reasoning flags;
- route audit metadata; started with safe `ResolvedInferenceRoute` fields for
  auth type, base URL source, policy decision, model catalog URL, and provider
  capabilities, plus credential source/kind/source-kind labels and route policy
  snapshots;
- model catalog listing with normalized model descriptors and safe credential
  source/kind/source-kind, auth type, and catalog URL source metadata;
- fallback routing for retryable failures with ordered route-attempt metadata;
- normalized usage and explicit request-priced cost metadata, including safe
  pricing source labels;
- provider and model allowlist policy hooks for project and organization rules;
  started with catalog filtering by required typed model capabilities and safe
  catalog policy metadata, provider-scoped auth type, credential
  kind/source-kind, and capability rules, plus enterprise cloud-proxy,
  custom-gateway, managed-identity, organization-credential, and direct agentic
  templates, sparse policy JSON serialization for control-plane defaults, and
  named built-in preset metadata, CLI preset selection, and shared TypeScript
  protocol/SDK preset exports for UI/config selection.

Why now: native provider execution is implemented, so routing quality is the
next bottleneck before cloud-managed providers and team usage controls.

Started enterprise credential groundwork with typed credential source kind
metadata, managed-identity credential injection, a resolver boundary for
managed identity token providers, and offline-tested Azure IMDS, AWS IMDSv2,
and GCP metadata server token acquisition, plus default-off live smoke test
scaffolding for cloud runtimes. Added enterprise organization-credential and
custom-gateway policy templates for team-managed provider credentials and
configured gateway rollouts, then made `ProviderPolicy` safely serializable for
cloud and desktop defaults, exposed stable built-in preset metadata, wired CLI
agent runs to select presets by ID, and mirrored the preset catalog through
TypeScript protocol/SDK packages for future cloud and desktop configuration
surfaces.

### 2. Memory 2.0

Goal: make project memory trusted, maintainable, and visibly sourced.

Build:

- memory editing and revision history; started with CLI/MCP-backed content
  edits that preserve metadata and append CLI/MCP-visible `edited` audit
  events with memory id and previous/new content metadata, plus CLI/MCP restore
  from edit audit revisions;
- deterministic approved-memory retrieval; started with CLI/MCP search over
  approved project memory by text, kind, tag, and limit;
- source evidence links; started with CLI detail text plus CLI
  list/show/search/similar JSON source session/turn metadata, replay
  `source_uri`, and MCP
  show/resource/list/search/similar JSON source metadata;
- duplicate and near-duplicate detection; started with exact active duplicate
  candidate rejection, repository-level active memory similarity scoring, and
  CLI/MCP review access to near-duplicate scores;
- stale-memory detection; started with repository-level newer active-memory
  overlap scoring for approved memories and CLI/MCP stale review output;
- visibility and sensitivity scopes; started with normal/sensitive memory
  labels, default normal-only retrieval, and explicit CLI/MCP sensitive-memory
  review/search filters;
- memory quality scoring; started with deterministic `MemoryRecord::quality()`
  advisory scores and findings for short, long, or vague memory content, now
  exposed in CLI/MCP memory JSON records for review surfaces, plus a core
  low-quality active-memory review query surfaced through CLI and MCP with
  status-, kind-, sensitivity-, and tag-scoped review filters.

Why next: memory is the product moat. Provider breadth matters, but durable
reviewed knowledge is what makes codel00p distinct for teams.

### 3. Agent Parity Hardening

Goal: close the remaining gap with mature coding agents.

Build:

- richer patch/diff engine;
- cancellation and interruption;
- background command monitoring;
- web fetch/search tools;
- worktree-isolated execution;
- PR preparation workflow;
- most-recent-session continue UX;
- deterministic context manifests.

### 4. MCP Certification

Goal: make external integrations predictable for real teams.

Build:

- compatibility matrix against common third-party MCP servers;
- regression fixtures for discovered edge cases;
- OAuth and authenticated connector flows;
- large tool-set search and filtering;
- connector policy templates.

### 5. Team Cloud And Sync

Goal: centralize project memory, provider policy, usage, permissions, and audit.

Build:

- organization, team, role, project, and invitation models;
- organization-managed provider credentials;
- shared memory sync;
- usage, cost, and budget views;
- cloud storage backend behind `codel00p-storage`;
- sync conflict representation and resolution.

### 6. Desktop And Cloud Interfaces

Goal: expose the agent, memory, and governance loop visually.

Shipped: a full-screen **terminal UI** (`ratatui`) — streaming transcript,
tool timeline, inline permission-approval modal, model picker, and a read-only
org entity browser. See [TUI initiative](initiatives/tui.md).

Next TUI slices (each independently shippable/testable):

- ~~backend `GET /org/members` route (Clerk Backend API) + `OrgMember` protocol
  type + `CloudClient::list_org_members` → real Users tab~~ **shipped
  2026-06-15** (`feat/tui-org-members-users-tab`);
- Clerk token re-mint in `login.rs` → **writable org switching** from the Org tab
  (today read-only; the stored token is scoped to one org);
- model picker sourced from a provider `list_models` call (today: static catalog
  in `tui/app.rs`);
- TUI mouse support, configurable themes, token/usage meters; session switcher.

Still to build (desktop/web):

- desktop session supervision timeline;
- desktop/web permission approval UI (terminal one shipped);
- memory candidate review queue;
- project knowledge browser;
- provider status and configuration views;
- team activity views.

### 7. Release & Self-Update (shipped — follow-ups)

Goal: let installed CLIs detect and install new releases instantly.

Shipped: `codel00p update` (SHA-256-verified in-place replace), daily background
check + nudge (CLI line + TUI header chip), `codel00p version`, tag-stamped
release binaries (`build.rs` + release.yml). Verified end to end v0.1.0 → v0.2.0.
**Release = bump `codel00p-cli` Cargo.toml version, commit, push a `vX.Y.Z` tag.**

Follow-ups:

- Windows self-update (today points at `install.ps1`);
- `cargo-dist` or similar to keep installer/updater/asset names in lockstep;
- release channels (stable/beta) once cadence justifies it.

## Selection Rule

Each loop cycle should choose the smallest slice that:

- advances the highest active priority;
- can be tested without live credentials unless explicitly marked as live-only;
- can be committed independently;
- leaves the repo closer to production use.
