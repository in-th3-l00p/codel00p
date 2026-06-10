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
candidate back to the originating agent turn before approving it.
MCP memory JSON includes the same values as `source.session_id` and
`source.turn_id` when source evidence is available.

## Edit audit contract

`MemoryRepository::edit` replaces memory content while preserving the memory
id, project, kind, status, source evidence, and tags. Empty replacement content
is rejected, and successful edits append an `edited` audit event with the actor
and optional reason. The CLI exposes this as:

```bash
codel00p memory edit <id> --actor <actor> --content <content> [--reason <reason>]
```

The MCP server exposes the same operation as the `memory_edit` tool with
`id`, `actor`, `content`, and optional `reason` arguments.
It also exposes `memory_audit` for machine-readable audit history.

Rich revision storage is still a separate Memory 2.0 follow-up.

## Review listing contract

`MemoryListFilter` lists memory records for human review. Unlike inference
retrieval, listing can return candidates, approved memories, rejected memories,
and archived memories. It supports project, status, kind, tag, and limit
filters, with deterministic ordering by memory id.

## Retrieval contract

`MemoryQuery` currently selects approved memory by project, with optional
filters for memory kind, tag, and text. Empty optional filters are ignored.
Results are sorted by memory id before `with_limit` is applied, which keeps
prompt-context construction deterministic across storage backends.

## SQLite backend

Enable the `sqlite` feature to use `codel00p-storage`'s SQLite backend with the
memory repository:

```bash
cargo test -p codel00p-memory --features sqlite
```

The feature test covers extracted candidates, review state, audit history, and
approved-memory retrieval across a reopened SQLite file.
