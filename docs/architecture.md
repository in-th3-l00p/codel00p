# Architecture

codel00p is planned as an agentic coding platform centered on shared project
knowledge. It should support both cloud and local runtimes, with cloud as the
strongest path for teams that want shared memory, governance, and provider
management.

At a high level:

```text
Developer / Team
  |
  | CLI / Desktop
  v
Agent harness
  |
  | uses
  v
Project memory + provider router + workspace tools
  |
  | sync / governance / provider policy
  v
Cloud organization platform
```

## Runtime

The agent runtime is the execution center of the system.

It should:

- run agent sessions against a software project;
- expose tools for repository work;
- retrieve relevant project memory;
- route inference requests to the selected provider;
- produce traceable session events;
- propose memory candidates after useful work.

The runtime may be local, cloud-connected, or eventually hosted. The important
contract is that agent work can use and improve reviewed project knowledge.

## Interfaces

codel00p will have two main developer interfaces.

The CLI is the first practical interface. It should support fast developer
workflows such as connecting a repo, starting a session, inspecting memory,
choosing a provider, and syncing approved memory with a team workspace.

The Electron app is the polished control center. It should make session
supervision, memory review, project navigation, and team activity easier to
understand than a terminal-only workflow.

Both interfaces should talk to the same harness and memory contracts.

## Cloud platform

The cloud platform is the team knowledge layer.

It should manage:

- organizations;
- teams;
- users and roles;
- projects;
- invitations;
- organization provider policy;
- shared memory sync;
- audit history and team activity.

The cloud platform should make codel00p stronger for organizations: shared
project memory, provider access, permissions, auditability, and cross-team
visibility all belong here.

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
- cloud and local operation;
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
