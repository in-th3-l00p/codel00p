# Subprojects

codel00p is split into smaller projects so the ecosystem can grow without
turning the final product into one large, unfocused codebase.

## codel00p-memory

The project memory engine.

Purpose:

- define the memory schema;
- store local project memory;
- extract memory candidates from completed work;
- support review, approval, editing, rejection, and deletion;
- retrieve relevant memory for future sessions;
- prepare approved memory for cloud sync.

Pitch:

> A durable memory layer for software projects, so AI agents and teammates stop
> relearning the same context.

This should be the first core subproject because memory is the product's
differentiator.

## codel00p-harness

The local agent runtime.

Purpose:

- run the agent loop;
- manage session lifecycle;
- execute tools in local workspaces;
- assemble context from files, tools, user input, and memory;
- route inference through selected providers;
- expose traceable session events;
- operate without cloud access.

Hermes Agent is the preferred reference candidate. The harness project should
evaluate whether codel00p should depend on Hermes directly, adapt it, fork it,
or build a custom harness.

Pitch:

> A local-first coding-agent harness that uses project memory and routes work
> across inference providers.

## codel00p-cli

The first developer-facing interface.

Purpose:

- connect a repository to codel00p;
- start and resume local agent sessions;
- select provider mode;
- inspect project memory;
- approve or reject memory candidates;
- trigger local/cloud sync when available.

Pitch:

> A terminal-native interface for running codel00p against real projects.

The CLI should ship before the desktop app because it is the fastest way to test
the memory and harness loop in real developer workflows.

## codel00p-desktop

The Electron control center.

Purpose:

- supervise agent sessions;
- review memory candidates;
- browse project knowledge;
- inspect provider settings;
- view team activity when cloud-connected;
- provide a polished local workspace for developers.

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

The cloud platform should add collaboration. It should not be required for
single-user local operation.

## codel00p-sync

The local/cloud synchronization layer.

Purpose:

- sync approved memory between local stores and organizations;
- preserve authorship and review metadata;
- handle offline changes;
- detect and resolve conflicts;
- enforce project and organization permissions.

Pitch:

> A sync layer for local-first project intelligence.

This may begin inside `codel00p-memory`, but it deserves a separate conceptual
boundary because synchronization has its own trust and permission concerns.

## codel00p-providers

The inference provider abstraction.

Purpose:

- adapt local credentials;
- adapt organization-provided credentials or proxy configuration;
- support model selection;
- expose budget and policy hooks;
- keep provider configuration separate from project memory.

Pitch:

> A provider router that lets teams choose where inference comes from: local,
> personal, organization-managed, or self-hosted.

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
- expose the complete CLI, desktop, local, and cloud-connected workflow.

Pitch:

> The final local-first agentic coding platform with durable project memory.

## Recommended release order

1. `codel00p-memory`
2. `codel00p-harness`
3. `codel00p-cli`
4. `codel00p-providers`
5. `codel00p-protocol`
6. `codel00p-desktop`
7. `codel00p-sync`
8. `codel00p-cloud`
9. `codel00p`

This order proves the memory and harness thesis before investing heavily in
desktop and cloud surfaces.
