# codel00p Architecture

codel00p is planned as a local-first agentic coding platform with optional cloud
collaboration. Its architecture is organized around a local agent harness, a
developer interface layer, and a cloud web platform for teams.

The main product promise is growing project memory: durable knowledge about a
company's projects that improves future agent sessions and helps teammates work
from shared context.

## System modules

### Agent harness

The agent harness is the runtime core. It owns the agent loop, tool execution,
provider selection, local project access, and memory retrieval.

Planned responsibilities:

- run agent sessions against a local workspace;
- expose tools for reading, editing, testing, searching, and inspecting projects;
- route inference requests to a selected provider;
- retrieve relevant project memory before and during work;
- suggest new memory entries from completed work;
- operate without a connection to the cloud platform.

The harness should treat the cloud as optional configuration and synchronization,
not as the runtime source of truth.

### Interface layer

The interface layer gives developers and teams a way to control the harness and
manage project knowledge.

Planned surfaces:

- **CLI:** fast local workflows for starting sessions, selecting providers,
  connecting projects, inspecting memory, and triggering sync.
- **Electron app:** a richer desktop interface for agent sessions, project
  dashboards, memory review, team activity, and project management.

The CLI should be the first practical interface because it keeps the earliest
implementation close to developer workflows. The Electron app can build on the
same local harness APIs once the runtime contracts are stable.

### Cloud web platform

The cloud platform adds organization-level collaboration.

Planned responsibilities:

- organization, team, project, role, and invitation management;
- organization-provided inference provider configuration;
- shared project memory sync;
- access control around projects and memory;
- team activity, audit history, and agent session visibility;
- billing-ready boundaries for future hosted services.

The cloud platform should not be required for single-user local operation.

## Operating modes

### Local-only mode

In local-only mode, the user runs the harness on their machine and configures
their own inference provider. Project memory is stored locally and used by local
agent sessions.

This mode is required for privacy-sensitive projects, offline-friendly workflows,
and developers who do not want organization-managed infrastructure.

### Cloud-connected mode

In cloud-connected mode, the local harness connects to an organization. The
organization may provide inference settings, team membership, project
configuration, and shared memory.

The local harness still executes work locally unless a future hosted execution
mode is intentionally added.

### Hybrid provider mode

In hybrid mode, a user can choose between local and organization-provided
inference options when organization policy allows it.

This lets teams balance cost, privacy, speed, and model quality on a
project-by-project or session-by-session basis.

## Project memory model

Project memory should be compact, reviewable, and useful. It should not be a raw
archive of every prompt, response, and terminal output.

Recommended memory categories:

- **Codebase facts:** module responsibilities, entry points, important paths,
  service boundaries, dependency notes.
- **Architecture decisions:** decisions, rationale, rejected alternatives, and
  consequences.
- **Workflows:** setup, test, deploy, debug, release, and rollback procedures.
- **Team conventions:** coding style, review expectations, naming preferences,
  documentation preferences.
- **Task outcomes:** summaries of important completed work and evidence that
  future agents should know.
- **Domain glossary:** business terms, product language, customer-specific
  concepts, and project vocabulary.

## Memory lifecycle

The memory lifecycle should make memory trustworthy:

1. **Observe:** agent sessions, developer actions, code changes, and team
   decisions produce candidate knowledge.
2. **Extract:** codel00p turns useful context into compact memory candidates.
3. **Review:** a developer or team member approves, edits, rejects, or scopes the
   candidate.
4. **Store:** approved memory is saved locally and optionally synced to the
   organization.
5. **Retrieve:** future agent sessions load relevant memory based on project,
   task, files, and user intent.
6. **Refine:** stale or low-value memory is corrected, merged, archived, or
   deleted.

Review is important. A memory system that stores everything will become noisy; a
memory system that stores only reviewed knowledge can become a real team asset.

## Provider routing

codel00p should support provider choice without binding the whole architecture
to one vendor.

Initial provider routing should support:

- user-owned local credentials;
- organization-provided credentials or proxy configuration;
- per-session provider selection;
- clear separation between provider configuration and project memory.

Provider policy belongs to the selected operating mode. In local-only mode, the
user controls the provider. In cloud-connected mode, the organization may expose
approved providers and restrictions.

## Hermes research spike

Hermes Agent is the preferred reference candidate for the harness, but the
integration strategy is not settled.

The research spike should evaluate four paths:

1. **Direct dependency:** use Hermes as the runtime foundation.
2. **Adapter:** keep codel00p interfaces stable while delegating execution to
   Hermes.
3. **Fork:** specialize Hermes where deeper control is required.
4. **Custom harness:** build a dedicated harness inspired by Hermes concepts.

Decision criteria:

- license compatibility;
- API and project stability;
- tool registration and execution model;
- provider routing flexibility;
- memory model compatibility;
- local-first operation;
- extension and plugin design;
- security boundaries for file and command access;
- observability and session history;
- maintenance cost.

Until the research spike is complete, public documentation should say that the
harness is based on or inspired by Hermes, not that Hermes is already integrated.

## Early implementation order

Recommended order:

1. Define the memory model and local storage format.
2. Research Hermes and decide dependency, adapter, fork, or custom harness.
3. Build a CLI prototype that can run local sessions and read/write memory.
4. Add provider selection for local credentials.
5. Add memory review flows.
6. Build the Electron app on top of stable local harness APIs.
7. Add cloud organization, team, provider policy, and memory sync features.

This order keeps the core local harness and memory loop useful before the cloud
platform exists.
