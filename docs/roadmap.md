# Roadmap

This roadmap turns the product direction in
[Product Roadmap](product-roadmap.md) into implementation milestones.

codel00p is building toward three combined capabilities:

- Claude Code-grade coding-agent execution;
- Hermes-grade provider routing breadth;
- a team control plane for project memory, provider policy, permissions, usage,
  and audit history.

## Milestone 1: Local Agent Foundation

Goal: prove that a local CLI agent can work in a repository and grow reviewed
project memory.

Implemented:

- `codel00p-memory` candidate creation, review lifecycle, audit history, and
  deterministic retrieval;
- `codel00p-storage` in-memory and SQLite backends;
- `codel00p-session` durable metadata and append-only replay;
- `codel00p-protocol` shared sessions, events, tool, provider, project, and
  memory contracts;
- `codel00p-providers` registry, profiles, policy, route inspection,
  credentials, OpenAI-compatible Chat Completions, tool calls, and live
  integration-test toggles;
- `codel00p-harness` session loop, provider adapter, memory injection,
  lifecycle hooks, project instructions, context compaction primitives,
  read-only tools, editing tools, shell command tool, git tools, permissions,
  streamable events, concurrency-safe tool batching, and explicit memory
  extraction;
- `codel00p-cli` agent runs, session resume, session inspection, memory review,
  provider-backed execution, JSON/streamed events, permission modes, tool-set
  opt-ins, MCP attachment, MCP diagnostics, and remembered MCP connector
  decisions;
- `codel00p-mcp` stdio/HTTP client transports, codel00p MCP server mode,
  tools, resources, prompts, logging, roots, pagination, subscriptions,
  reconnects, diagnostics, and harness event routing.

Next:

- richer patch/diff engine;
- cancellation and interruption;
- background command monitoring;
- web fetch/search tools;
- worktree-isolated execution;
- PR preparation workflow;
- most-recent session continue command;
- deterministic context manifests.

Exit criteria:

- a user can ask codel00p to inspect, edit, test, commit, and resume work in a
  real repository with safe defaults and durable memory review.

## Milestone 2: Provider Platform

Goal: make inference routing broad, policy-aware, and enterprise-friendly.

Build:

- OpenAI Responses transport;
- AWS Bedrock Converse transport;
- Gemini native transport;
- Azure AI Foundry deployment-aware configuration;
- GitHub Copilot and GitHub Models hardening;
- model catalogs and model capability metadata;
- normalized usage, cost, cache, and reasoning metadata;
- fallback routing;
- organization provider policy hooks;
- cloud proxy routing.

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
- merge/split workflows;
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

Build:

- organization, team, user, role, project, and invitation models;
- shared project memory sync;
- provider policy and organization credentials;
- permission policy management;
- usage, cost, and budget views;
- audit history and activity feeds;
- sync conflict representation and resolution;
- cloud storage backend behind `codel00p-storage`.

Exit criteria:

- a team can share reviewed project memory and provider settings across its
  engineering workflow with visible usage and audit controls.

## Milestone 6: Desktop And Cloud Interfaces

Goal: expose the agent, memory, and governance workflow through polished user
interfaces.

Build:

- desktop session supervision timeline;
- permission approval UI;
- memory candidate review queue;
- project knowledge browser;
- provider status and configuration views;
- team activity views;
- cloud sign-in and workspace switching;
- shared protocol event rendering across CLI, desktop, and cloud.

Exit criteria:

- users can understand and control codel00p visually while CLI workflows remain
  scriptable.

## Milestone 7: Multi-Agent Work

Goal: coordinate long-horizon work across focused agents without losing memory,
policy, or auditability.

Build:

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

- codel00p feels like one product instead of a collection of modules.
