# codel00p-memory

Project memory engine for codel00p.

This crate owns candidate creation, review lifecycle, storage-backed audit
history, and deterministic retrieval for approved project knowledge.

## Retrieval contract

`MemoryQuery` currently selects approved memory by project, with optional
filters for memory kind, tag, and text. Empty optional filters are ignored.
Results are sorted by memory id before `with_limit` is applied, which keeps
prompt-context construction deterministic across storage backends.
