# Agentic Backlog

This backlog is the working queue for the agentic loop. It is ordered by current
leverage, not by final product importance.

## Active Priority

### 1. Provider Route Intelligence

Goal: make provider routing policy-aware, auditable, and ready for team control.

Build:

- model capability metadata;
- route audit metadata; started with safe `ResolvedInferenceRoute` fields for
  base URL source, policy decision, model catalog URL, and provider
  capabilities;
- fallback routing for retryable failures with ordered route-attempt metadata;
- normalized usage and cost metadata;
- provider policy hooks for project and organization rules.

Why now: native provider execution is implemented, so routing quality is the
next bottleneck before cloud-managed providers and team usage controls.

### 2. Memory 2.0

Goal: make project memory trusted, maintainable, and visibly sourced.

Build:

- memory editing and revision history;
- source evidence links;
- duplicate and near-duplicate detection;
- stale-memory detection;
- visibility and sensitivity scopes;
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
