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

The first runtime implementation is `codel00p-harness`, a Rust crate that runs
deterministic read-only agent turns and adapts them to `codel00p-providers`.
See [Agent Harness](harness.md) for the current harness design.

External tool integration starts in `codel00p-mcp`, a transport-neutral Rust
crate that adapts MCP server tools into harness tools named
`mcp.<server>.<tool>`. Keeping MCP behind the same harness tool registry means
CLI, desktop, and cloud runtimes can share permission checks, event streams, and
audit behavior for external systems. The first runtime transports are stdio and
HTTP: workspace `.codel00p/mcp.json` files and ad hoc CLI flags can launch local
MCP server processes or connect to remote MCP endpoints, discover tools, assign
permission scopes, and inspect tools without a model call.
The CLI can also run `codel00p mcp serve`, a stdio MCP server exposing read-only
project memory and session replay tools to other agents.
Ask-mode MCP connector decisions can be remembered in the same project-scoped
local store, keyed by tool name and permission scope, so trusted connectors do
not require repeated prompts.

Durable persistence is split into two layers. `codel00p-storage` owns the
backend-neutral storage primitives: scoped key-value state, structured
documents, and ordered append logs. `codel00p-session` owns session-specific
metadata, lineage, transcript records, and replay semantics on top of those
primitives.

Services must not call SQLite, Redis, cloud storage, or object stores directly.
They should expose typed domain repositories and persist through
`codel00p-storage`. The first backend is in-memory for contract tests. SQLite,
Redis, and cloud storage should arrive as additional backends behind the same
capability-based API.

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

The provider layer should be implemented in Rust as `codel00p-providers`.
Hermes Agent is the reference for breadth and transport shape: provider profiles
describe identity, auth, endpoints, and quirks, while transports handle the wire
protocol for Chat Completions, Anthropic Messages, Responses, Bedrock Converse,
Gemini, and external-process paths. codel00p should own this contract directly
so the harness, CLI, desktop app, and cloud platform all share the same routing,
policy, credential, usage, and audit semantics.

See [Inference Providers](providers.md) for the initial provider list and
implementation design.

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
- `protocol` defines shared data contracts for sessions, turns, events, tools,
  provider references, projects, and memory entries;
- `storage` defines backend-neutral local and cloud persistence primitives;
- `session` persists durable transcripts, boundary events, lineage, and replay
  records on top of storage;
- `cli` and `desktop` expose user interfaces;
- `sync` coordinates local/cloud knowledge;
- `cloud` manages organizations, teams, policies, and shared state.

The final `codel00p` product integrates these modules.
