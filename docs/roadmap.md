# Roadmap

This roadmap turns the product direction in
[Product Roadmap](product-roadmap.md) into implementation milestones.

codel00p is building toward three combined capabilities:

- Claude Code-grade coding-agent execution;
- Hermes-grade provider routing breadth;
- a team control plane for project memory, provider policy, permissions, usage,
  and audit history.

The [Initiatives](initiatives/README.md) directory plans the Hermes-distinctive
capabilities and maps each to the milestones below. **Phase 1 of most has
shipped** (plugins & hooks, skills, self-improvement loop, sub-agent delegation,
scheduling/cron, messaging gateway, and the TUI); only sandboxed execution
backends and programmatic tool calling are unstarted. Start with the
[Hermes Gap Analysis](initiatives/hermes-gap-analysis.md).

## Milestone 1: Local Agent Foundation

Goal: prove that a local CLI agent can work in a repository and grow reviewed
project memory.

Implemented:

- `codel00p-memory` candidate creation, review lifecycle, audit history, and
  deterministic retrieval;
- `codel00p-storage` in-memory and SQLite backends with document, key-value,
  and append-log primitives plus collection listing;
- `codel00p-session` durable metadata, append-only replay, and session listing;
- `codel00p-protocol` shared sessions, events, tool, provider, project, and
  memory contracts;
- `codel00p-providers` registry, profiles, policy, route inspection,
  credentials, OpenAI-compatible Chat Completions, Azure AI Foundry deployment
  Chat Completions, OpenAI Responses, Anthropic Messages, AWS Bedrock Converse,
  Gemini GenerateContent, native token streaming for every transport
  (server-sent events plus the Bedrock binary event stream), tool calls,
  fallback routing, model catalog listing, request-priced cost estimates,
  provider/model allowlist policy, and live integration-test toggles;
- `@codel00p/protocol-ts` and `@codel00p/sdk` provider policy preset metadata
  exports for cloud and desktop configuration surfaces;
- `codel00p-harness` session loop, provider adapter, memory injection,
  lifecycle hooks, project instructions, context compaction primitives,
  read-only tools, editing tools, shell command tool, git tools, permissions,
  streamable events, token-streaming model-client boundary, concurrency-safe
  tool batching, and explicit memory extraction;
- `codel00p-cli` layered TOML configuration (`config` and `providers` commands,
  user + project files under `~/.codel00p`, `.env` credential storage) so flags
  become optional overrides, agent runs, session resume, interactive multi-turn
  chat sessions with resumable history, in-session tools (sessions, history,
  tools, model switching, memory), and live token streaming, conversation
  listing, session inspection, memory review, provider-backed execution,
  JSON/streamed events, permission modes, tool-set opt-ins, MCP attachment, MCP
  diagnostics, and remembered MCP connector decisions;
- `codel00p-mcp` stdio/HTTP client transports, codel00p MCP server mode,
  tools, resources, prompts, logging, roots, pagination, subscriptions,
  reconnects, diagnostics, and harness event routing.

Recently shipped:

- **most-recent session continue command** (`codel00p agent continue <prompt>`):
  resumes the newest session by stamping a `created_at` on session metadata —
  the codel00p analogue of Claude Code's `--continue`.
- **cancellation and interruption**: Ctrl-C during `agent run` stops the turn at
  the next boundary via a cooperative `CancelSignal`, persisting partial progress
  instead of killing the process (TUI Esc-to-cancel is a follow-up).
- **recency-ordered session listing**: `session list` (and the chat sessions
  overlay) now sort most-recent-first by `created_at`, with the timestamp also in
  `session list --json` — the companion to `agent continue`.
- **TUI chat rework**: wrap-aware transcript scrolling (PageUp/PageDown + mouse
  wheel, with follow-the-tail) that fixes the clipped-bottom overflow bug; an
  editable composer with a real cursor (←/→, Home/End, Delete, Alt+Enter newline)
  that wraps and grows instead of overflowing; and a redesigned, role-labeled
  transcript (colored accent bars + `You`/`codel00p` headers).
- **Hermes-inspired chat polish**: assistant messages render **Markdown** (fenced
  code blocks, headings, lists, blockquotes, inline bold/italic/code); the status
  spinner cycles thinking/tool **verbs** (`pondering` → `reading`…); the empty
  composer shows rotating placeholder hints and the empty transcript a welcome
  banner — modeled on the Hermes `ui-tui` chat.

Next:

- richer patch/diff engine;
- background command monitoring;
- web fetch/search tools;
- worktree-isolated execution;
- PR preparation workflow;
- deterministic context manifests.

Exit criteria:

- a user can ask codel00p to inspect, edit, test, commit, and resume work in a
  real repository with safe defaults and durable memory review.

## Milestone 2: Provider Platform

Goal: make inference routing broad, policy-aware, and enterprise-friendly.

Build:

- provider catalog metadata surfaced in policy templates; started with an
  enterprise direct agentic catalog template, a cloud-proxy route template, and
  a managed-identity credential template;
- richer provider-specific catalog metadata and model capability metadata;
- managed pricing catalog distribution and refresh;
- richer provider reasoning and cache metadata;
- cloud proxy management;
- cloud and desktop provider policy preset selection backed by the shared
  TypeScript preset catalog.

Exit criteria:

- the same harness and CLI can route through personal credentials,
  organization-managed credentials, or a gateway without changing memory,
  sessions, or tool contracts.

## Milestone 3: Memory 2.0

Goal: make reviewed project memory accurate, maintainable, and valuable for
teams.

Build:

- memory editing and revision history;
- source evidence links;
- duplicate and near-duplicate detection;
- stale-memory detection;
- sensitivity and visibility scopes;
- merge/split workflows; started with `memory merge` (archive the duplicate,
  carry its tags onto the survivor, two-sided `merged` audit trail);
- semantic retrieval behind deterministic filters;
- memory quality scoring;
- post-session memory recommendations;
- imports from docs, issues, PRs, and existing project notes.

Exit criteria:

- a team can review, correct, trust, and share memory without polluting future
  agent context.

## Milestone 4: MCP Certification

Goal: make external tool integration practical for real teams.

Current baseline:

- see [MCP Compatibility](mcp-compatibility.md).

Build:

- live certification against filesystem, git, GitHub, Linear, Jira, Postgres,
  SQLite gateways, Playwright, Figma, Sentry, Grafana, and internal HTTP MCP
  gateways;
- OAuth and authenticated connector flows;
- large tool-set search/filtering;
- connector policy templates;
- remote HTTP MCP gateway hardening;
- codel00p MCP tools for agent/session control after policy design.

Exit criteria:

- common MCP servers work predictably, and every discovered edge case becomes a
  deterministic regression fixture.

## Milestone 5: Team Cloud And Sync

Goal: let organizations manage AI usage, project memory, providers, and audit
history centrally.

Implemented:

- organization, team, user, role, and project models via **Clerk
  Organizations**; the `codel00p-cloud` axum service keys product data by Clerk
  org/user IDs and verifies session JWTs (RS256 via JWKS) at the boundary;
- full org-scoped CRUD for projects, agents, and MCP server configs plus a
  reviewed-memory queue, exposed over HTTP and the typed `@codel00p/sdk`;
- shared project-memory sync (`codel00p cloud push|pull|status`) and stored
  agent runs resolved into RunPlans (`codel00p cloud run`);
- a **Postgres storage backend** behind `codel00p-storage`, deployed and
  **verified durable in production** (survives restarts; Fly.io `codel00p-cloud`
  + `codel00p-db`);
- live updates over server-sent events (`GET /events`) so desktop and web
  reflect changes without polling.

Still to build:

- provider policy and organization-managed credentials end to end;
- permission policy management;
- usage, cost, and budget views;
- organization-wide audit history and activity feeds;
- sync conflict representation and resolution.

Exit criteria:

- a team can share reviewed project memory and provider settings across its
  engineering workflow with visible usage and audit controls.

## Milestone 6: Desktop And Cloud Interfaces

Goal: expose the agent, memory, and governance workflow through polished user
interfaces.

Implemented:

- an Electron **desktop control center** with Clerk sign-in, a live
  organization dashboard (projects, agents, MCP servers), custom dark account
  UI, and an install view when the `codel00p` binary is absent;
- a memory candidate **review queue** (cloud service + web dashboard);
- **cloud sign-in across every surface**: web `/sign-in` + dashboard, the
  desktop OAuth handoff, and the `codel00p login` browser flow for the CLI
  (localhost loopback + Clerk `cli` JWT template, credentials at
  `~/.codel00p/credentials.toml`);
- **live protocol event rendering** with end-to-end token streaming in the CLI
  and desktop; bare `codel00p` opens the interactive chat.
- a **full-screen terminal UI** (`ratatui`): streaming transcript, tool-call
  timeline, **inline permission-approval modal**, model picker, and a read-only
  org **entity browser** (projects, agents, MCP servers, memory, **a real Users
  roster** backed by `GET /org/members` over the Clerk Backend API, org/role).
  See [TUI initiative](initiatives/tui.md). Non-TTY/CI falls back to the line REPL.

Still to build:

- desktop session supervision timeline;
- desktop/web permission approval UI (the **terminal** approval modal shipped);
- TUI phase 3: writable org switching (Clerk token re-mint), mouse, themes, and
  usage meters (the read-only Users tab shipped; see
  [TUI initiative](initiatives/tui.md));
- a richer project knowledge browser;
- provider status and configuration views;
- team activity views and workspace switching.

Exit criteria:

- users can understand and control codel00p visually while CLI workflows remain
  scriptable.

## Milestone 7: Multi-Agent Work

Goal: coordinate long-horizon work across focused agents without losing memory,
policy, or auditability.

Implemented:

- synchronous **sub-agent delegation** (`codel00p-harness` `delegation` module):
  a `delegate_task` tool, typed agent roles, a real spawner with read-only
  child tool sets, a concurrency semaphore, and parent/child session lineage.
  See the [Sub-Agents & Delegation initiative](initiatives/subagents-delegation.md).

Still to build:

- research, implementation, review, and memory-extraction agents;
- worktree isolation for mutating subagents;
- parallel task execution;
- durable task plans;
- resumable long-running goals;
- team-visible task state;
- review agents that can block or annotate risky work.

Exit criteria:

- codel00p can split large tasks across focused agents and preserve a coherent
  session, memory, and audit record.

## Milestone 8: Production Release

Goal: package codel00p as a cohesive open-source product with an enterprise
cloud path.

Started:

- **hosted cloud deployment**: `codel00p-cloud` runs on Fly.io (`iad`, always-on)
  with a managed Postgres database, auto-deployed from `main` via GitHub Actions
  and fronted by the `codel00p` Vercel web app;
- **prebuilt release binaries and a curl installer** for distribution;
- a **self-updater**: `codel00p update` pulls the matching release asset, verifies
  its SHA-256, and replaces the running binary; a throttled daily background check
  nudges users (CLI line + TUI header chip); `codel00p version` reports the build.
  Released binaries are tag-stamped via `build.rs` + `CODEL00P_RELEASE_VERSION`
  (release.yml). **Release process**: bump `core/crates/codel00p-cli/Cargo.toml`
  version, commit, push a `vX.Y.Z` tag — verified end to end (v0.1.0 → v0.2.0).

Still to build:

- Windows self-update (today it points users at `install.ps1`);
- migrations and compatibility policy;
- security review and sandboxing;
- privacy controls;
- eval suite for agent, provider, memory, and MCP behavior;
- a docs site;
- release channels (stable/beta).

Exit criteria:

- codel00p feels like one product instead of a collection of modules.
