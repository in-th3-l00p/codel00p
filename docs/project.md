# Project Description

codel00p is a local-first agentic coding platform for teams that want AI agents
to understand their real projects over time.

The product is not just a coding agent. It is a system for building, reviewing,
retrieving, and sharing project knowledge across human developers and agent
sessions.

## Core thesis

The strongest long-term advantage of an agentic coding system is not only model
quality. It is the quality of the surrounding harness, tools, context, memory,
and team workflow.

codel00p should make those parts explicit:

- the harness runs agent work;
- the memory layer preserves useful project knowledge;
- the interfaces help developers supervise work and curate memory;
- the cloud layer coordinates teams, providers, and shared governance;
- the final product brings these pieces together into one polished experience.

## Product principles

### Local-first

The local harness must be useful without cloud access. A developer should be
able to connect a repository, configure a provider, run agent sessions, and grow
project memory on their own machine.

### Memory-centered

Memory is the product's main differentiator. codel00p should optimize for
high-quality reviewed memory, not raw transcript storage.

### Provider-flexible

Inference should be configurable. A user may use local credentials, an
organization-provided provider, or a future self-hosted/local model endpoint.

### Modular

Each subproject should have a clear responsibility and a stable interface. The
final `codel00p` product should integrate modules that are independently useful.

### Open-source by default

The project should be understandable, hackable, and useful without closed
infrastructure. Cloud features can add hosted collaboration, but they should not
make the local system unusable.

## Final product

The polished `codel00p` product should provide:

- a local agent harness for repository work;
- a CLI for fast developer workflows;
- a desktop app for session supervision and memory review;
- a cloud platform for organizations and teams;
- provider routing for local and organization-managed inference;
- project memory that improves over time.

The first practical milestone is smaller: prove that local project memory can
make agent sessions better.
