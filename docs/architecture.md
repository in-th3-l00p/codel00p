# Architecture

codel00p is planned as a local-first agentic coding platform with optional cloud
collaboration.

At a high level:

```text
Developer
  |
  | CLI / Desktop
  v
Local agent harness
  |
  | uses
  v
Project memory + provider router + workspace tools
  |
  | optional sync
  v
Cloud organization platform
```

## Local runtime

The local runtime is the center of the system.

It should:

- run agent sessions against a local workspace;
- expose tools for repository work;
- retrieve relevant project memory;
- route inference requests to the selected provider;
- produce traceable session events;
- propose memory candidates after useful work;
- continue working without cloud access.

The cloud platform should never be required for the basic local workflow.

## Interfaces

codel00p will have two main local interfaces.

The CLI is the first practical interface. It should support fast developer
workflows such as connecting a repo, starting a session, inspecting memory,
choosing a provider, and syncing approved memory.

The Electron app is the polished local control center. It should make session
supervision, memory review, project navigation, and team activity easier to
understand than a terminal-only workflow.

Both interfaces should talk to the same local harness contracts.

## Cloud platform

The cloud platform adds team coordination.

It should manage:

- organizations;
- teams;
- users and roles;
- projects;
- invitations;
- organization provider policy;
- shared memory sync;
- audit history and team activity.

The cloud platform should provide governance and collaboration. It should not
replace the local harness as the default runtime.

## Provider routing

Inference may come from:

- user-owned local credentials;
- organization-managed credentials;
- a proxy configured by the organization;
- future self-hosted or local model endpoints.

Provider configuration must stay separate from project memory. A project memory
entry should remain useful even if the team changes model providers.

## Hermes decision

The harness is intended to be based on or inspired by Hermes Agent, but the
integration strategy is deliberately unresolved.

The research project should evaluate:

- direct dependency;
- adapter;
- fork;
- custom harness inspired by Hermes.

Decision criteria:

- license compatibility;
- API stability;
- tool execution model;
- provider routing flexibility;
- memory integration;
- local-first operation;
- extension design;
- security boundaries;
- observability;
- long-term maintenance cost.

Until that research is complete, documentation should avoid claiming that Hermes
is already integrated.

## Boundaries

The clean boundary is:

- `memory` stores and retrieves project knowledge;
- `harness` runs agent sessions;
- `providers` routes inference;
- `protocol` defines shared data contracts;
- `cli` and `desktop` expose user interfaces;
- `sync` coordinates local/cloud knowledge;
- `cloud` manages organizations, teams, policies, and shared state.

The final `codel00p` product integrates these modules.
