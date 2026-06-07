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

## Retrieval contract

`MemoryQuery` currently selects approved memory by project, with optional
filters for memory kind, tag, and text. Empty optional filters are ignored.
Results are sorted by memory id before `with_limit` is applied, which keeps
prompt-context construction deterministic across storage backends.
