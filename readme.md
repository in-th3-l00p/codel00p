# codel00p

codel00p is an early-stage blueprint for a local-first agentic coding platform.
It is designed around one core idea: **your project memory should grow as your
team works**.

Most coding agents wake up with limited context. codel00p aims to make the
context durable. It should help a team collect, review, reuse, and improve
knowledge about its codebases, architecture, decisions, workflows, conventions,
and previous work so future agent sessions start from a stronger foundation.

## What codel00p is meant to become

codel00p will be made of three main modules:

1. **Agent harness**
   - Runs the agent loop.
   - Connects to inference providers.
   - Executes tools against local projects.
   - Retrieves and updates project memory.
   - Works locally without requiring the cloud platform.

2. **Interface layer**
   - Provides a CLI for direct developer workflows.
   - Provides an Electron app for richer project, memory, and team workflows.
   - Connects users to the local harness.
   - Helps create, inspect, and refine project knowledge.

3. **Cloud web platform**
   - Manages organizations, teams, projects, roles, and access.
   - Lets organizations provide shared inference configuration.
   - Synchronizes reviewed project memory across a team.
   - Gives teams a shared place to coordinate agent-assisted work.

## Local-first by default

The agent harness should be useful without a cloud account.

A developer should be able to run codel00p locally, connect it to a project,
configure a local or personal inference provider, and build memory for that
project on their machine.

The cloud platform should add collaboration, not become a hard dependency. When
connected to a cloud organization, a user should be able to use organization
providers, sync approved memory, and participate in team-level project
management. When disconnected, the local harness should continue to work.

## Provider flexibility

codel00p should not force one inference provider.

Inference may come from:

- a provider configured locally by the user;
- a provider made available by the user's cloud organization;
- a future local model or self-hosted endpoint.

The user should be able to choose the available option that fits the project,
budget, privacy needs, and organization policy.

## Project memory

Project memory is the central differentiator.

codel00p should help teams preserve knowledge such as:

- how the codebase is structured;
- which modules own which responsibilities;
- important architecture decisions and their rationale;
- setup, deployment, and debugging workflows;
- team conventions and review preferences;
- recurring errors and how they were fixed;
- product terminology and domain context;
- outcomes from previous agent sessions.

The goal is not to save raw chat history forever. The goal is to turn useful
work into compact, reviewable, reusable project knowledge.

## Hermes harness research

The harness is intended to be based on, or inspired by, Hermes Agent. Before
locking the architecture, codel00p needs a research spike to decide whether to:

- use the Hermes repository directly;
- build an adapter around Hermes;
- fork and specialize Hermes;
- build a custom harness from scratch using Hermes as a reference architecture.

The decision should consider licensing, API stability, extensibility, provider
routing, memory integration, local execution, security boundaries, and long-term
maintainability.

## Planned documentation

- [Architecture](docs/architecture.md) - the planned system modules, local/cloud
  operating modes, memory lifecycle, and Hermes research path.

## Status

codel00p is currently an early blueprint. The first milestone is to make the
architecture clear enough that the harness, interface, cloud platform, and
memory model can be implemented deliberately rather than improvised.
