# codel00p-memory Development Plan

`codel00p-memory` is the first subproject because durable project knowledge is
the product's core advantage. The first implementation should be small, strict,
well-tested, and useful before it becomes clever.

The goal is to build a memory engine that can be embedded by the harness, CLI,
desktop app, and cloud platform without making those layers depend on each
other.

## Engineering principles

### Memory is reviewed knowledge, not logs

The engine must not become a transcript database. Raw sessions, tool output,
prompts, model responses, and traces may be inputs to memory extraction, but
approved memory should be compact, reviewed, source-aware, and reusable.

### Project knowledge must survive provider changes

Memory entries must not depend on Anthropic, OpenAI, Gemini, Hermes, or any
other model-specific behavior. A memory entry should remain useful if a team
changes provider, harness, cloud workspace, or UI.

### Strong contracts before intelligence

The first version should define reliable schemas, validation, lifecycle states,
retrieval interfaces, and test fixtures before adding advanced extraction or
ranking. Poorly shaped memory is harder to fix later than a missing feature.

### Small modules with explicit boundaries

Each module should have one responsibility:

- schema defines what memory is;
- storage persists and queries memory;
- lifecycle manages candidate, review, approval, archive, and deletion states;
- retrieval selects relevant memory for a task;
- extraction proposes candidates from session artifacts;
- sync prepares approved memory for organization sharing;
- audit records who changed knowledge and why.

### Determinism first, ranking second

Initial retrieval should be explainable and deterministic: project scope,
category, path, tags, recency, and explicit relevance should work before opaque
semantic ranking. Embeddings and rerankers can improve retrieval later, but
they should not be required for the first useful release.

### Security and privacy are data-model concerns

Memory must carry scope and visibility from the beginning. A team should be able
to tell whether an entry belongs to a repository, module, organization, user,
team, or private workspace. Sensitive facts must be rejectable, redacted, or
scoped before sync.

### Documentation is part of the API

Every public type, lifecycle transition, storage behavior, and retrieval rule
should be documented near the code and summarized in repository docs. The memory
engine is only useful if contributors and users understand what the system is
allowed to remember.

## Stack

The core memory engine is implemented in Rust (`codel00p-memory`).

Rust is a good fit because the memory engine needs strict data contracts,
portable binaries, predictable performance, and embeddability across the CLI,
harness, desktop app, and cloud services.

As built, the module layout is:

```text
core/crates/codel00p-memory/
  src/
    lib.rs
    error.rs
    extraction.rs   // candidate extraction (e.g. `remember:` directives)
    inputs.rs       // MemoryCandidateInput, extraction inputs
    query.rs        // list/retrieval/similarity/staleness/quality query builders
    records.rs      // MemoryRecord, quality scoring
    repository.rs   // the MemoryRepository trait + storage-backed handle
    review.rs       // ReviewDecision, MemoryEdit, MemoryMerge, audit events
    store.rs        // storage-backed repository implementation
    util.rs
```

Initial dependencies should stay conservative:

- `serde` and `serde_json` for stable serialization;
- `thiserror` for typed errors;
- `uuid` or ULID-style IDs for durable identifiers;
- `chrono` or `time` for timestamps;
- `rusqlite` only if SQLite is selected for the first embedded store;
- `tantivy` or SQLite FTS only after the basic storage contract is proven.

The first version should not require a vector database.

## Core domain types

The domain concepts, as implemented (names reflect the current code):

- `MemoryEntry` (in `codel00p-protocol`): a durable memory record with a
  `MemoryStatus` (`candidate`/`approved`/`rejected`/`archived`).
- `MemoryCandidateInput`: proposed knowledge submitted for review.
- `MemoryKind`: architecture, convention, workflow, decision, deployment, or
  troubleshooting. (The doc's earlier `MemoryCategory` name; the broader
  `MemoryScope` of organization/repository/module/path is **not** implemented —
  scoping is by `ProjectRef` plus `MemorySensitivity`.)
- `MemorySource`: where the knowledge came from (a session/turn, optionally with
  a source URI).
- `ReviewDecision` + `MemoryEdit` + `MemoryMerge`: the review, edit, and merge
  actions (the doc's earlier `MemoryReview`), recorded as `MemoryAuditEvent`s.
- `MemoryQuery` (and the `*Query`/`*Filter` builders in `query.rs`): retrieval
  requests from the harness or interface.
- `RetrievedMemory`: selected memory plus score, reason, and source metadata.

The schema should make review state explicit. An unreviewed candidate is not
the same thing as approved memory.

## Test-driven development

`codel00p-memory` should be built with TDD from the first commit.

Every feature should follow this loop:

1. Write the smallest failing test that describes the behavior.
2. Run the exact test and confirm it fails for the expected reason.
3. Implement the smallest code that makes the test pass.
4. Run the exact test again.
5. Run the relevant module test suite.
6. Update docs or examples if the public behavior changed.
7. Commit the focused change.

The test suite should include:

- unit tests for schema validation and lifecycle transitions;
- storage tests using temporary repositories or temporary databases;
- retrieval tests with fixed fixtures and deterministic expected results;
- serialization compatibility tests for public JSON shapes;
- redaction and visibility tests for sensitive memory;
- sync payload tests for approved memory only;
- regression tests for every bug fixed.

TDD matters here because memory failures are often subtle. A bad transition,
leaky scope, duplicate entry, or nondeterministic retrieval result can poison a
team's shared knowledge over time.

## Documentation standards

Documentation should be written at three levels.

Code-level docs:

- every public type explains what it represents;
- every lifecycle transition explains when it is allowed;
- every error type explains what the caller should do;
- examples show valid construction and expected failure cases.

Repository docs:

- explain memory concepts in `docs/memory.md`;
- explain implementation approach in this file;
- maintain schema examples in `docs/schemas/memory-entry.json` once the first
  schema is stable;
- maintain retrieval examples in `docs/examples/memory-retrieval.md`;
- document migration notes whenever persisted data changes.

Contributor docs:

- describe how to run tests;
- describe how to add a memory category;
- describe how to change storage safely;
- describe the compatibility policy for serialized memory.

## Initial implementation phases

### Phase 1: Schema and lifecycle

Build the Rust crate, public domain types, validation rules, typed errors, and
candidate-to-approved lifecycle. The output should be an in-memory library with
tests and JSON serialization.

Status: started in `core/crates/codel00p-memory`. The first implementation
supports candidate creation, explicit review transitions, audit logs, storage
backing through `codel00p-storage`, and deterministic retrieval of approved
memory. Retrieval now supports project-scoped filtering by memory kind, tag,
and text, plus deterministic result limits for prompt-context assembly. The
crate also includes an explicit extractor that turns `remember ...:` directives
from summaries or human notes into candidate memory without requiring model
extraction.

Success criteria:

- candidates can be created, edited, approved, rejected, archived, and deleted;
- approved entries are immutable except through explicit revision actions;
- invalid scope, empty content, missing source, and invalid transitions fail
  with typed errors;
- JSON fixtures round-trip without data loss.

### Phase 2: Embedded storage

Add a simple local store for candidates, approved entries, revisions, and audit
events. SQLite is the likely first storage engine because it is portable,
inspectable, transactional, and easy to ship with a CLI.

Success criteria:

- a repository can initialize a memory store;
- entries and candidates survive process restarts;
- revisions and review events are stored with authorship and timestamps;
- tests run against temporary stores without global state.

### Phase 3: Deterministic retrieval

Add retrieval by project, scope, category, tags, path, source, and simple text
matching. Return explanations for why each memory item was selected.

Success criteria:

- a harness can request relevant memory for a task;
- retrieval is deterministic for fixed fixtures;
- returned memory includes reason strings and source metadata;
- users can inspect what was loaded.

### Phase 4: Memory extraction

Build extraction interfaces that propose reviewable candidates from completed
turns, session summaries, tool outputs, imported documents, and human notes.

Status: started. `ExplicitMemoryExtractor` supports deterministic extraction
from explicit `remember ...:` directives, including optional kind and tags.
Every extracted item remains a candidate until it passes the normal review
lifecycle. `codel00p-harness` can now run an explicit turn extractor after a
completed turn and persist candidates through a `MemoryCandidateSink`; failures
are non-fatal and candidates are never approved automatically.

Success criteria:

- extraction can run without writing directly to approved memory;
- extracted candidates include source session and turn metadata;
- deterministic extractors are available for tests and human-authored notes;
- model-assisted extractors can be added behind the same trait later.

### Phase 4: Candidate extraction interface

Define the interface for proposing memory candidates from session summaries,
tool traces, human notes, and code review outcomes. The first implementation can
be rule-based and manually triggered.

Success criteria:

- extraction produces candidates, not approved memory;
- candidates include source evidence;
- users can edit before approval;
- duplicate or near-duplicate candidates are detectable.

### Phase 5: Sync-ready payloads

Prepare approved memory for organization sharing without implementing the full
cloud sync system. Sync payloads should include scope, visibility, revisions,
source metadata, and audit information.

Success criteria:

- only approved memory is syncable;
- private and user-scoped entries can be excluded;
- payloads are stable JSON;
- conflict inputs are represented without needing a final conflict resolver.

### Phase 6: Quality review signals

Add deterministic quality signals that help humans prioritize memory cleanup.

Status: started. `MemoryRecord::quality()` returns a 0-100 advisory score with
stable findings for memory that is too short to reuse, too long for frequent
retrieval, or written with vague language. `MemoryRepository::quality_review`
now exposes a deterministic low-quality active-memory queue ordered by score
and memory id.

Success criteria:

- quality scoring is deterministic for fixed content;
- findings are stable enough for CLI, MCP, desktop, and cloud surfaces;
- scoring never approves, rejects, archives, or hides memory by itself;
- future semantic or model-assisted scoring can layer on top of the core signal.

## Quality gates

Before any memory feature is considered complete:

- tests must pass locally;
- public types must have docs;
- persisted JSON changes must include fixtures;
- lifecycle changes must include transition tests;
- retrieval changes must include deterministic fixture tests;
- security-sensitive changes must include visibility or redaction tests;
- docs must explain the behavior at the right level.

## First milestone

The first useful milestone is not an autonomous memory generator. It is a
trusted memory library that can create, validate, review, store, retrieve, and
explain durable project knowledge.

Once that exists, the harness and CLI can integrate it without guessing what
"memory" means.
