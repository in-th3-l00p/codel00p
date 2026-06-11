# codel00p-memory

Project memory engine for codel00p.

This crate owns candidate creation, review lifecycle, storage-backed audit
history, and deterministic retrieval for approved project knowledge.

## Extraction contract

`ExplicitMemoryExtractor` creates reviewable candidates from explicit lines in
session summaries or human notes:

```text
remember architecture[harness,runtime]: The harness owns tool execution.
remember workflow[verify]: Run pnpm verify before pushing main.
remember: The team prefers small focused commits.
```

The default kind is `decision`. Unknown kinds, empty content, and ordinary prose
are ignored. Extracted candidates use deterministic IDs based on source session,
source turn, and extracted candidate order.

## Source evidence

Candidates keep the source session and turn that produced them. The CLI
`memory show` command prints this source evidence so reviewers can trace a
candidate back to the originating agent turn before approving it, including a
`source_uri` replay link such as `codel00p://sessions/<session-id>`. CLI
`memory list --json` and `memory show <id> --json` expose the same source
fields as scriptable record objects.
MCP memory JSON, including show/resource/list/search/similar responses,
includes the same values as `source.session_id`, `source.turn_id`, and
`source_uri` when source evidence is available.

## Sensitivity contract

Memory records default to `normal` sensitivity. Review clients can opt a
candidate into `sensitive` sensitivity before approval. Sensitive approved
memory stays visible to review surfaces, but default `MemoryQuery` retrieval
excludes it so ordinary inference context does not receive sensitive project
knowledge accidentally.

Explicit retrieval can request `MemorySensitivity::Sensitive`. CLI
`memory search --sensitivity sensitive` and MCP `memory_search` with
`sensitivity: "sensitive"` expose that path. CLI/MCP JSON record objects include
the `sensitivity` label, and `memory list`/`memory_list` can filter by it.

## Edit audit contract

`MemoryRepository::edit` replaces memory content while preserving the memory
id, project, kind, status, source evidence, sensitivity, and tags. Empty
replacement content is rejected, and successful edits append an `edited` audit
event with the actor and optional reason. Edit audit events also preserve
machine-readable `previous_content` and `new_content` revision fields.

The CLI exposes edits as:

```bash
codel00p memory edit <id> --actor <actor> --content <content> [--reason <reason>]
codel00p memory audit <id> --json
codel00p memory restore <id> --sequence <audit-sequence> --actor <actor> [--reason <reason>]
```

The MCP server exposes the same operation as the `memory_edit` tool with
`id`, `actor`, `content`, and optional `reason` arguments.
The CLI `memory audit <id> --json` command and MCP `memory_audit` tool expose
machine-readable audit history, including `memory_id` and edit revision content
when available.
The CLI `memory restore` command and MCP `memory_restore` tool use an edit
audit event's `previous_content` to write a new auditable edit that restores
older content.

Richer revision browsing is still a separate Memory 2.0 follow-up.

## Duplicate detection contract

Candidate creation rejects exact duplicates of active project memory. A new
candidate is a duplicate when an existing candidate or approved memory in the
same project has the same kind and same trimmed content. Rejected and archived
memories do not block replacement candidates.

`MemoryRepository::similar_active` scores proposed memory content against active
candidate and approved memory in the same project and kind. It returns
deterministic token-overlap scores for review workflows without changing exact
duplicate rejection semantics. The CLI exposes this as `memory similar` with
stable TSV output and `memory similar --json` for scored record objects. The
MCP server exposes the same scored records through `memory_similar`.

`MemoryRepository::stale_active` flags approved memory that may have been
superseded by newer active candidate or approved memory in the same project and
kind. It uses the same deterministic token-overlap score and ignores
archived/rejected newer records. The CLI exposes this as `memory stale` with
stable TSV output and `memory stale --json` for stale records with a nested
newer memory object. The MCP server exposes the same stale review queue through
`memory_stale`.

## Review listing contract

`MemoryListFilter` lists memory records for human review. Unlike inference
retrieval, listing can return candidates, approved memories, rejected memories,
archived memories, and sensitive memories. It supports project, status, kind,
sensitivity, tag, and limit filters, with deterministic ordering by memory id.

## Retrieval contract

`MemoryQuery` currently selects approved normal-sensitivity memory by project,
with optional filters for memory kind, sensitivity, tag, and text. Empty
optional filters are ignored. Results are sorted by memory id before
`with_limit` is applied, which keeps prompt-context construction deterministic
across storage backends.
The CLI exposes this as `memory search` with stable TSV output and
`memory search --json` for the same machine-readable records returned by the
MCP `memory_search` tool.

## SQLite backend

Enable the `sqlite` feature to use `codel00p-storage`'s SQLite backend with the
memory repository:

```bash
cargo test -p codel00p-memory --features sqlite
```

The feature test covers extracted candidates, review state, audit history, and
approved-memory retrieval across a reopened SQLite file.

## Quality scoring contract

`MemoryRecord::quality()` returns a deterministic `MemoryQuality` score from
0 to 100 plus stable findings for review surfaces. The first heuristic penalizes
memory that is too short to reuse, too long for frequent retrieval, or written
with vague language. The score is advisory only: it does not block candidate
creation, approval, retrieval, or sync.
