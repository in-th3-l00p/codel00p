# Project Memory

Project memory is the central differentiator of codel00p.

The goal is not to store every chat, prompt, response, or terminal output. The
goal is to preserve useful knowledge in a compact, reviewed, reusable form.
That knowledge may live in a cloud workspace, a local runtime, or both. The
important thing is that it becomes durable, reviewable, and available where the
team works.

## What memory should capture

Project memory should capture knowledge such as:

- repository structure and module responsibilities;
- important paths and entry points;
- architecture decisions and rationale;
- rejected alternatives;
- setup, test, deployment, and rollback workflows;
- debugging procedures;
- recurring errors and fixes;
- team conventions;
- review preferences;
- product and domain terminology;
- summaries of important completed tasks.

## Memory categories

Recommended categories:

- **Codebase facts:** stable facts about files, modules, services, and ownership.
- **Architecture decisions:** decisions, rationale, consequences, and tradeoffs.
- **Workflows:** repeated operational procedures.
- **Team conventions:** style, review, naming, and process preferences.
- **Task outcomes:** useful summaries from completed work.
- **Domain glossary:** business and product language.

## Lifecycle

Memory should move through a deliberate lifecycle:

1. **Observe:** agent sessions and developer work produce candidate knowledge.
2. **Extract:** codel00p converts useful context into memory candidates.
3. **Review:** a human approves, edits, scopes, or rejects each candidate.
4. **Store:** approved memory is saved in the active workspace.
5. **Sync:** approved memory can be shared across an organization.
6. **Retrieve:** future sessions load relevant memory for the current task.
7. **Refine:** stale or low-value memory is corrected, merged, archived, or
   deleted.

Review is required because unreviewed memory becomes noise quickly.

The harness exposes lifecycle hooks for this flow:

- `on_turn_started` can queue recall for the next turn;
- `on_pre_inference` can inject reviewed context or update context pressure;
- `on_post_tool` can observe evidence without blocking tool execution;
- `on_pre_compact` can extract facts before older transcript segments are
  summarized;
- `on_turn_completed` can queue durable memory extraction at safe boundaries.

Memory extraction should run only when token and tool-call thresholds justify
it, prefer natural breaks after final assistant answers, and use restricted
permissions when a background extractor writes memory candidates.

## Storage and sync

The memory system should support both cloud and local storage. Cloud storage is
the strongest path for teams because it makes reviewed knowledge available
across developers, projects, and agent sessions.

Storage should prioritize:

- readability;
- portability;
- easy review;
- deterministic retrieval;
- simple backup and versioning;
- permission-aware sharing.

Local storage is still useful for private work, offline operation, and
self-managed setups. The product should not make memory quality depend on where
the runtime happens to execute.

## Retrieval principles

Memory retrieval should be:

- relevant to the current project;
- relevant to the current task;
- scoped by file, module, workflow, or team context where possible;
- compact enough to fit into agent context;
- transparent enough that users can inspect what was loaded.

The user should be able to understand why memory was used and correct memory
that is stale or wrong.

## Quality scoring

Memory review surfaces should help reviewers find low-value entries before
they pollute future context. The core memory engine now exposes a deterministic
per-record quality score with findings for short, overly long, or vague memory
content. CLI and MCP JSON memory records include this score as
`quality.score` with `quality.findings`. This score is advisory, not an
approval gate.

The core memory repository also exposes a low-quality review query for active
candidate or approved memory. Rejected and archived records remain auditable,
but they do not appear in this cleanup queue.

## Prompt assembly

Approved memory enters inference through a provider-neutral system message
assembled by the harness:

```text
Project memory:
- id: mem-harness
  kind: architecture
  tags: harness,runtime
  reason: matched tag harness
  content: The harness owns tool execution.
```

Only approved memory can reach this path. Candidates, rejected memory, and
archived memory remain available for review/audit workflows but are excluded
from inference retrieval.

## Development approach

The first implementation should be `codel00p-memory`: a Rust memory engine with
strict schemas, explicit lifecycle transitions, deterministic retrieval, and
test-driven development from the first commit.

See [codel00p-memory Development Plan](memory-development.md) for the detailed
engineering approach.

## Quality bar

Good memory is:

- short;
- specific;
- reviewed;
- source-aware;
- easy to update;
- useful across future sessions.

Bad memory is:

- vague;
- duplicated;
- stale;
- copied directly from raw chat;
- too long to retrieve often;
- impossible to verify.
