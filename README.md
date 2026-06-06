# codel00p

codel00p is an open-source, local-first agentic coding platform built around a
simple premise: **a team's project memory should grow as the team works**.

The final codel00p product will combine a local agent harness, CLI, desktop app,
cloud collaboration platform, provider routing, and shared project memory into a
polished developer experience. The project is intentionally split into smaller
subprojects so each layer can be designed, tested, and released independently.

## Why this exists

Coding agents are useful, but most sessions start with weak project context.
They relearn the same repository structure, conventions, deployment details,
debugging patterns, and decisions again and again.

codel00p is focused on making that knowledge durable. It should help teams turn
real work into compact, reviewed, reusable memory that improves future agent
sessions and gives teammates a shared understanding of the codebase.

## Subprojects

The codel00p ecosystem is planned as a set of open-source projects:

- **codel00p-memory:** project memory engine.
- **codel00p-harness:** local agent runtime.
- **codel00p-cli:** first developer-facing interface.
- **codel00p-desktop:** Electron control center.
- **codel00p-cloud:** organization and team platform.
- **codel00p-sync:** local/cloud memory synchronization.
- **codel00p-providers:** inference provider abstraction.
- **codel00p-protocol:** shared contracts between modules.
- **codel00p-research:** harness and memory experiments.
- **codel00p:** the final integrated product.

See [Subprojects](docs/subprojects.md) for the full technical split.

## Operating model

codel00p is local-first:

- the harness should run without a cloud account;
- project memory should work locally;
- users should be able to configure their own inference provider;
- cloud services should add team sync, provider policy, organization management,
  and shared governance.

Cloud-connected teams can use organization-managed providers and synchronized
project memory. Local users should still be able to work independently.

## Documentation

- [Project Description](docs/project.md)
- [Subprojects](docs/subprojects.md)
- [Architecture](docs/architecture.md)
- [Project Memory](docs/memory.md)
- [Roadmap](docs/roadmap.md)
- [Contributing](CONTRIBUTING.md)

## Current status

codel00p is in the blueprint stage. The immediate goal is to make the technical
direction clear enough for focused implementation, starting with project memory,
the local harness, and the CLI.
