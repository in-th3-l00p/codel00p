# Subprojects

codel00p is split into smaller projects so the ecosystem can grow without
turning the final product into one large, unfocused codebase.

## codel00p-memory

The project memory engine.

Purpose:

- define the memory schema;
- store project memory;
- extract memory candidates from completed work;
- support review, approval, editing, rejection, and deletion;
- retrieve relevant memory for future sessions;
- prepare approved memory for workspace sync.

Pitch:

> A durable memory layer for software projects, so AI agents and teammates stop
> relearning the same context.

This should be the first core subproject because memory is the product's
differentiator.

The first Rust implementation exists at
[`core/crates/codel00p-memory`](../core/crates/codel00p-memory). It currently
defines candidate creation, explicit review transitions, audit logs, and
deterministic retrieval for approved memory.

## codel00p-harness

The agent runtime.

Purpose:

- run the agent loop;
- manage session lifecycle;
- execute tools in project workspaces;
- assemble context from files, tools, user input, and memory;
- route inference through selected providers;
- expose traceable session events;
- support cloud-connected and local execution modes.

Hermes Agent is the preferred reference candidate. The harness project should
evaluate whether codel00p should depend on Hermes directly, adapt it, fork it,
or build a custom harness.

Pitch:

> A coding-agent harness that uses project memory and routes work across
> inference providers.

## codel00p-cli

The first developer-facing interface.

Purpose:

- connect a repository to codel00p;
- start and resume agent sessions;
- select provider mode;
- inspect project memory;
- approve or reject memory candidates;
- sync approved memory with a team workspace when available.

Pitch:

> A terminal-native interface for running codel00p against real projects.

The CLI should ship before the desktop app because it is the fastest way to test
the memory and harness loop in real developer workflows.

The first Rust implementation exists at
[`core/crates/codel00p-cli`](../core/crates/codel00p-cli). It currently provides
SQLite-backed memory review commands for listing, showing, approving, rejecting,
archiving, and auditing project memory records.

## codel00p-desktop

The Electron control center.

Purpose:

- supervise agent sessions;
- review memory candidates;
- browse project knowledge;
- inspect provider settings;
- view team activity;
- provide a polished workspace for developers.

Pitch:

> A desktop workspace for supervising agents and curating the memory of your
> codebase.

## codel00p-cloud

The organization and team platform.

Purpose:

- manage organizations, teams, users, roles, projects, and invitations;
- provide organization-managed inference configuration;
- sync approved project memory;
- enforce access control;
- expose audit history and team activity;
- prepare billing and hosted-service boundaries.

Pitch:

> The organization layer for shared project memory, team governance, and
> provider management.

The cloud platform is a core product surface for organizations because shared
memory, provider access, permissions, and audit history are team concerns.

## codel00p-sync

The memory synchronization layer.

Purpose:

- sync approved memory between workspaces, devices, and organizations;
- preserve authorship and review metadata;
- handle offline changes;
- detect and resolve conflicts;
- enforce project and organization permissions.

Pitch:

> A sync layer for shared project intelligence.

This may begin inside `codel00p-memory`, but it deserves a separate conceptual
boundary because synchronization has its own trust, conflict, and permission
concerns.

## codel00p-providers

The Rust inference provider abstraction.

Purpose:

- adapt user-owned credentials, organization-provided credentials, and cloud
  proxy configuration;
- support model selection;
- expose budget and policy hooks;
- keep provider configuration separate from project memory.

Pitch:

> A provider router that lets teams choose where inference comes from: cloud,
> personal, organization-managed, or self-hosted.

The first supported providers should focus on corporate engineering adoption:
Anthropic, OpenAI, Azure AI Foundry, AWS Bedrock, Google Gemini, GitHub
Copilot/GitHub Models, OpenRouter, and custom OpenAI-compatible endpoints. See
[Inference Providers](providers.md) for the Rust technical design.

## codel00p-protocol

The shared contract layer.

Purpose:

- define session event formats;
- define memory candidate formats;
- define tool call and tool result formats;
- define provider configuration shapes;
- define project identity and sync payload contracts.

Pitch:

> The shared protocol that lets codel00p modules evolve independently.

This project prevents the CLI, desktop app, harness, memory layer, and cloud
platform from inventing incompatible data shapes.

The first Rust implementation exists at
[`core/crates/codel00p-protocol`](../core/crates/codel00p-protocol). It currently defines
stable contracts for sessions, turns, events, tool calls, provider references,
projects, and memory entries.

## codel00p-storage

The backend-neutral persistence layer.

Purpose:

- keep services from depending on SQLite, Redis, cloud APIs, or object stores
  directly;
- provide scoped key-value storage for settings, cursors, leases, and runtime
  state;
- provide document storage for structured project, session, memory, and team
  records;
- provide append-log storage for transcripts, memory evolution, audit history,
  and sync streams;
- let local and cloud storage backends expose the same clean API.

Pitch:

> A storage boundary that lets codel00p swap local and cloud backends without
> rewriting service logic.

The first Rust implementation exists at
[`core/crates/codel00p-storage`](../core/crates/codel00p-storage). It currently defines
the storage traits, an in-memory backend used by `codel00p-session` tests, and
a feature-gated SQLite backend for durable local project state.

## codel00p-research

The experimental lab.

Purpose:

- evaluate Hermes integration paths;
- benchmark harness behavior;
- test memory retrieval quality;
- compare provider behavior;
- analyze traces and task outcomes;
- keep experimental work separate from production modules.

Pitch:

> The lab where codel00p proves which harness and memory designs actually work.

## codel00p

The integrated product.

Purpose:

- package the stable modules into a polished developer experience;
- provide the main user-facing distribution;
- coordinate releases across the ecosystem;
- expose the complete CLI, desktop, cloud, and runtime workflow.

Pitch:

> The final agentic coding platform with durable project memory for teams.

## Recommended release order

1. `codel00p-memory`
2. `codel00p-harness`
3. `codel00p-cli`
4. `codel00p-providers`
5. `codel00p-protocol`
6. `codel00p-storage`
7. `codel00p-desktop`
8. `codel00p-sync`
9. `codel00p-cloud`
10. `codel00p`

This order proves the memory and harness thesis while still treating cloud as a
first-class path for team-scale value.
