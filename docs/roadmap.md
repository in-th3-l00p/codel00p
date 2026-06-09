# Roadmap

The roadmap is ordered to prove the knowledge and memory thesis while building
toward a strong team product.

## Phase 1: Memory foundation

Goal: define and validate useful project memory.

Work:

- design the memory schema;
- implement the first Rust `codel00p-memory` crate;
- follow test-driven development for schema, lifecycle, storage, and retrieval;
- choose initial storage and sync boundaries;
- define memory categories;
- build review and edit flows;
- test retrieval against real repositories.

Success means a user or team can connect a project, add reviewed memory, and
retrieve useful context for future work.

## Phase 2: Harness research and runtime

Goal: decide the Hermes strategy and build the first agent runtime.

Work:

- evaluate Hermes as dependency, adapter target, fork, or reference;
- define harness contracts;
- run agent sessions;
- connect workspace tools;
- inject project memory into sessions;
- emit traceable session events.

Success means the harness can run useful work with memory-aware context.

## Phase 3: CLI

Goal: ship the first usable developer surface.

Implemented:

- run one read-only agent turn through `codel00p agent run`;
- call Chat-Completions-compatible providers with environment credentials;
- inspect memory;
- approve or reject memory candidates;
- persist and inspect session records;
- extract candidate memory from completed turns;
- inject approved memory into later provider requests.

Remaining work:

- connect repositories;
- resume sessions;
- configure user or organization providers;
- expose basic sync hooks for later phases.

Success means early users can use codel00p from the terminal and grow reviewed
project memory across repeated agent sessions.

## Phase 3.5: Coding agent parity

Goal: close the practical gap with mature coding agents while preserving
codel00p's reviewed-memory advantage.

Work:

- resume and continue persisted sessions;
- load project instructions from codel00p-native files and compatibility files;
- add permissioned editing tools;
- add permissioned shell execution;
- add git inspection, diff review, commit, and PR preparation workflows;
- add MCP support for external team tools: stdio/HTTP client transports, CLI
  workspace config, remembered connector decisions, and read-only codel00p MCP
  server mode are in place; policy management commands and broader server tools
  are next;
- add context compaction and session summaries;
- stream events for interactive clients.

Success means a user can ask codel00p to implement a real repository change,
watch the agent inspect, edit, test, and commit safely, and then review the
project knowledge learned during the work.

## Phase 4: Providers and protocol

Goal: stabilize contracts between modules.

Work:

- implement the Rust provider contract;
- add initial corporate providers: Anthropic, OpenAI, Azure AI Foundry, AWS
  Bedrock, Google Gemini, GitHub Copilot/GitHub Models, OpenRouter, and custom
  OpenAI-compatible endpoints;
- define session events;
- define provider config shapes;
- define memory candidate shapes;
- add provider adapters;
- separate local and organization-managed provider settings.

Success means CLI, harness, desktop, cloud, and sync can evolve without
conflicting data formats.

## Phase 5: Desktop app

Goal: build the polished developer control center.

Work:

- supervise agent sessions;
- review memory candidates;
- browse project memory;
- show provider state;
- prepare team knowledge views.

Success means users can understand and control codel00p visually.

## Phase 6: Cloud and sync

Goal: add organization-scale knowledge and governance.

Work:

- organization and team management;
- project membership;
- shared memory sync;
- provider policy;
- audit history;
- conflict handling;
- permissions.

Success means a team can share reviewed project memory and provider settings
across its engineering workflow.

## Phase 7: Integrated codel00p

Goal: release the polished product.

Work:

- integrate stable modules;
- simplify setup;
- provide cohesive docs;
- package CLI and desktop workflows;
- coordinate cloud and runtime features;
- define release channels.

Success means codel00p feels like one product instead of a collection of
experiments.
