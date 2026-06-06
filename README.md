# codel00p

codel00p is an open-source agentic coding platform built around a simple
premise: **a team's project knowledge should grow as the team works**.

The final codel00p product will combine an agent harness, CLI, desktop app,
cloud platform, provider routing, and shared project memory into a polished
developer experience. The project is intentionally split into smaller
subprojects so each layer can be designed, tested, and released independently.

## Why this exists

Coding agents are useful, but most sessions start with weak project context.
They relearn the same repository structure, conventions, deployment details,
debugging patterns, and decisions again and again.

codel00p is focused on making that knowledge durable. It helps teams turn real
work into compact, reviewed, reusable memory that improves future agent
sessions and gives teammates a shared understanding of the codebase.

Cloud matters because team knowledge is more powerful when it is shared. The
cloud platform should make project memory available across an organization,
centralize provider access, and give teams governance over how agents are used
in real engineering workflows.

## Subprojects

The codel00p ecosystem is planned as a set of open-source projects:

- **codel00p-memory:** project memory engine.
- **codel00p-harness:** agent runtime.
- **codel00p-cli:** first developer-facing interface.
- **codel00p-desktop:** Electron control center.
- **codel00p-cloud:** organization and team platform.
- **codel00p-sync:** memory synchronization.
- **codel00p-providers:** inference provider abstraction.
- **codel00p-protocol:** shared contracts between modules.
- **codel00p-research:** harness and memory experiments.
- **codel00p:** the final integrated product.

See [Subprojects](docs/subprojects.md) for the full technical split.

## Operating model

codel00p should support cloud and local operation:

- cloud workspaces provide shared memory, organization governance, provider
  access, and team visibility;
- local runtimes support private or offline work when teams need it;
- users and organizations can choose the provider strategy that fits their
  project, cost, privacy, and compliance needs.

The product should not force an ideology around where agents run. The durable
asset is the project knowledge that codel00p captures and makes reusable.

## Documentation

- [Project Description](docs/project.md)
- [Subprojects](docs/subprojects.md)
- [Architecture](docs/architecture.md)
- [Inference Providers](docs/providers.md)
- [Agent Harness](docs/harness.md)
- [Project Memory](docs/memory.md)
- [Memory Development Plan](docs/memory-development.md)
- [Roadmap](docs/roadmap.md)
- [Contributing](CONTRIBUTING.md)

## Current implementation

The first Rust modules have started:

- [codel00p-providers](crates/codel00p-providers): provider registry,
  high-level inference client, OpenAI-compatible Chat Completions transport,
  tool calls, route inspection, and provider policy enforcement.
- [codel00p-harness](crates/codel00p-harness): read-only agent turn loop,
  workspace-safe tools, deterministic events, model-client boundary, and
  provider adapter.

## Current status

codel00p is in early implementation. The immediate goal is to connect provider
inference, the read-only harness, project memory, and the CLI into a usable
developer workflow.
