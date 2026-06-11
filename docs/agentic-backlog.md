# Agentic Backlog

This backlog is the working queue for the agentic loop. It is ordered by current
leverage, not by final product importance.

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
  managed-identity, and direct agentic templates.

Why now: native provider execution is implemented, so routing quality is the
next bottleneck before cloud-managed providers and team usage controls.

Started enterprise credential groundwork with typed credential source kind
metadata, managed-identity credential injection, a resolver boundary for
managed identity token providers, and offline-tested Azure IMDS, AWS IMDSv2,
and GCP metadata server token acquisition, plus default-off live smoke test
scaffolding for cloud runtimes.

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
- memory quality scoring.

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

Build:

- desktop session supervision timeline;
- permission approval UI;
- memory candidate review queue;
- project knowledge browser;
- provider status and configuration views;
- team activity views.

## Selection Rule

Each loop cycle should choose the smallest slice that:

- advances the highest active priority;
- can be tested without live credentials unless explicitly marked as live-only;
- can be committed independently;
- leaves the repo closer to production use.
