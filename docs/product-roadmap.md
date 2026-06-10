# Product Roadmap

codel00p is aiming at a specific product shape:

> Claude Code-grade coding-agent execution, Hermes-grade provider breadth, and
> a team control plane for project memory, AI usage, providers, permissions, and
> auditability.

The point is not to clone an existing coding agent. The point is to make a
coding agent that gets stronger as a team uses it because its useful project
knowledge becomes reviewed, durable, and shareable.

## Product Pillars

### Agent execution

codel00p must be able to do real repository work:

- inspect and edit files;
- run tests, formatters, linters, and builds;
- inspect git state, create clean commits, and prepare PR-ready summaries;
- resume sessions;
- load project instructions;
- stream typed events to CLI, desktop, cloud, and automation clients;
- manage context pressure and compact long sessions;
- connect to external systems through MCP;
- eventually coordinate focused subagents for long-horizon work.

### Provider breadth

Hermes is the reference for provider breadth and transport shape. codel00p
should own the Rust provider contract while supporting the providers corporate
teams actually approve:

- Anthropic;
- OpenAI;
- Azure AI Foundry;
- AWS Bedrock;
- Google Gemini;
- GitHub Copilot and GitHub Models as separate provider profiles;
- OpenRouter;
- custom OpenAI-compatible gateways;
- future self-hosted or local endpoints.

Provider routing must carry credential source, policy, usage, audit, and
fallback information without coupling project memory to any model vendor.

### Project memory

Project memory is the moat. The platform should turn completed work into
reviewed reusable knowledge:

- codebase facts;
- architecture decisions;
- workflows;
- team conventions;
- task outcomes;
- domain glossary entries;
- debugging procedures;
- setup, deploy, rollback, and test knowledge.

Only approved memory should reach future inference context. Unreviewed
candidates remain available for review, editing, rejection, audit, and sync.

### Team control plane

The cloud and desktop surfaces should let a team manage how AI is used across a
project:

- organizations, teams, users, roles, projects, and invitations;
- organization-managed provider access;
- per-project provider policy;
- permission policy for native and MCP tools;
- shared memory review queues;
- memory sync and conflict handling;
- session history and audit logs;
- usage, cost, and provider routing visibility;
- team activity around agents and memory.

## Roadmap Stages

### Stage 1: Agent parity foundation

Status: in progress, with many core pieces already implemented.

Current foundation:

- provider-backed CLI agent runs;
- session persistence and resume;
- read-only, editing, shell, git, and MCP tool surfaces;
- project instructions from `CODEL00P.md`, `AGENTS.md`, and `CLAUDE.md`;
- permission policy and remembered MCP connector decisions;
- streamable protocol events;
- context compaction primitives;
- memory extraction and approved-memory injection.

Next work:

- replace exact-string `apply_patch` with a richer patch/diff engine;
- add cancellation and interruption;
- add background command monitoring;
- add web fetch/search tools;
- add worktree-isolated execution;
- add PR preparation workflow;
- improve most-recent-session continue UX;
- add deterministic context manifests for debugging.

Exit criteria:

- a user can ask codel00p to inspect, edit, test, commit, and resume work in a
  real repository with safe defaults and clear audit output.

### Stage 2: Provider platform

Status: started.

Current foundation:

- provider profiles, aliases, registry lookup, credential resolution, route
  inspection, allowlist policy, OpenAI-compatible Chat Completions transport,
  Azure AI Foundry deployment Chat Completions transport, Anthropic Messages
  transport, OpenAI Responses transport, AWS Bedrock Converse transport, Gemini
  GenerateContent transport, tool calls, normalized responses, model catalog
  listing with typed descriptions, capabilities, modalities, and token limits,
  GitHub Copilot and GitHub Models profile split, GitHub Models live smoke-test
  coverage, fallback routing, request-priced cost estimates, provider/model
  allowlist policy, enterprise-direct policy template, client-injected
  provider/model pricing, published provider pricing catalogs, client-level
  cloud proxy routing, expanded cache/reasoning usage metadata, and live
  integration-test toggles.

Next work:

- provider-specific catalog annotations.

Exit criteria:

- CLI, desktop, cloud, and harness can route inference through user-owned,
  organization-managed, or gateway-backed providers with consistent policy and
  audit semantics.

### Stage 3: Memory 2.0

Status: started.

Current foundation:

- candidate creation;
- explicit review lifecycle;
- audit history;
- deterministic approved-memory retrieval;
- explicit memory extraction from completed turns;
- SQLite-backed local persistence through `codel00p-storage`;
- CLI review commands.

Next work:

- memory editing and revision history;
- source evidence links;
- duplicate and near-duplicate detection;
- stale-memory detection;
- visibility and sensitivity scopes;
- merge/split workflows;
- semantic retrieval behind deterministic filters;
- memory quality scoring;
- memory recommendations after agent work;
- import from docs, issues, PRs, and existing project notes.

Exit criteria:

- teams can trust codel00p to recommend useful memory, review it efficiently,
  retrieve it transparently, and keep stale or sensitive knowledge under
  control.

### Stage 4: MCP ecosystem

Status: strong baseline implemented.

Current foundation:

- stdio and HTTP MCP clients;
- JSON and SSE responses;
- lifecycle initialization;
- tools, resources, resource templates, prompts, logging, roots, pagination,
  subscriptions, reconnects, diagnostics, remembered permissions, and harness
  event routing;
- codel00p MCP server for memory and sessions.

Next work:

- live certification against common third-party MCP servers;
- OAuth and authenticated connector flows;
- remote HTTP MCP gateway hardening;
- large tool-set search and filtering;
- connector policy templates;
- codel00p MCP tools for agent/session control after policy design.

Exit criteria:

- teams can connect external systems such as GitHub, Linear, Jira, Postgres,
  Playwright, Sentry, and internal HTTP MCP gateways with predictable policy,
  diagnostics, and regression fixtures.

### Stage 5: Team cloud and sync

Status: planned.

Build:

- organization, team, user, role, project, and invitation models;
- shared project memory sync;
- provider policy and organization credentials;
- audit history and activity feeds;
- usage, cost, and budget views;
- permission policy management;
- sync conflict representation and resolution;
- cloud storage backend behind `codel00p-storage`.

Exit criteria:

- a team can centrally manage project memory, provider access, AI usage, and
  audit history while developers keep using CLI or desktop workflows.

### Stage 6: Desktop and polished interfaces

Status: scaffolded.

Build:

- session supervision timeline;
- permission approval UI;
- memory candidate review queue;
- project knowledge browser;
- provider status and configuration views;
- team activity views;
- cloud sign-in and workspace switching;
- shared protocol event rendering across CLI, desktop, and cloud.

Exit criteria:

- developers can understand and control agent work visually without giving up
  terminal workflows.

### Stage 7: Multi-agent work

Status: planned.

Build:

- focused research, implementation, review, and memory-extraction agents;
- worktree isolation for mutating subagents;
- parallel task execution;
- durable task plans;
- resumable long-running goals;
- team-visible task state;
- review agents that can block or annotate risky work.

Exit criteria:

- codel00p can coordinate larger project changes while preserving session
  history, memory review, permissions, and auditability.

### Stage 8: Production release

Status: planned.

Build:

- installers and package distribution;
- migrations and compatibility policy;
- security review and sandboxing;
- privacy controls;
- eval suite for agent, provider, memory, and MCP behavior;
- docs site;
- release channels;
- hosted cloud deployment.

Exit criteria:

- codel00p feels like one product instead of a collection of modules, and teams
  can adopt it with a clear operational model.

## Prioritized Next Moves

1. Align docs and status so contributors know what is already built.
2. Harden provider breadth: provider catalogs, fallback routing, and
   organization policy.
3. Improve memory quality: evidence, revision, duplicate, and stale handling.
4. Certify MCP against real third-party servers.
5. Add worktree isolation, richer patching, cancellation, and web tools.
6. Start cloud data contracts for organization, team, project, provider policy,
   usage, and shared memory sync.
