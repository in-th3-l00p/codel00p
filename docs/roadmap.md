# Roadmap

The roadmap is ordered to prove the memory thesis before building the full
desktop and cloud product.

## Phase 1: Memory foundation

Goal: define and validate useful local project memory.

Work:

- design the memory schema;
- choose a local storage format;
- define memory categories;
- build review and edit flows;
- test retrieval against real repositories.

Success means a user can connect a project, add reviewed memory, and retrieve
useful context for future work.

## Phase 2: Harness research and runtime

Goal: decide the Hermes strategy and build the first local agent runtime.

Work:

- evaluate Hermes as dependency, adapter target, fork, or reference;
- define harness contracts;
- run local agent sessions;
- connect workspace tools;
- inject project memory into sessions;
- emit traceable session events.

Success means the harness can run useful local work with memory-aware context.

## Phase 3: CLI

Goal: ship the first usable developer surface.

Work:

- connect repositories;
- start and resume sessions;
- inspect memory;
- approve or reject memory candidates;
- configure local providers;
- expose basic sync hooks for later phases.

Success means early users can use codel00p from the terminal.

## Phase 4: Providers and protocol

Goal: stabilize contracts between modules.

Work:

- define session events;
- define provider config shapes;
- define memory candidate shapes;
- add provider adapters;
- separate local and organization-managed provider settings.

Success means CLI, harness, desktop, cloud, and sync can evolve without
conflicting data formats.

## Phase 5: Desktop app

Goal: build the polished local control center.

Work:

- supervise agent sessions;
- review memory candidates;
- browse project memory;
- show provider state;
- prepare cloud-connected team views.

Success means users can understand and control codel00p visually.

## Phase 6: Sync and cloud

Goal: add organization collaboration.

Work:

- organization and team management;
- project membership;
- shared memory sync;
- provider policy;
- audit history;
- conflict handling;
- permissions.

Success means a team can share reviewed project memory and provider settings
without losing local-first operation.

## Phase 7: Integrated codel00p

Goal: release the polished product.

Work:

- integrate stable modules;
- simplify setup;
- provide cohesive docs;
- package CLI and desktop workflows;
- coordinate cloud-connected features;
- define release channels.

Success means codel00p feels like one product instead of a collection of
experiments.
