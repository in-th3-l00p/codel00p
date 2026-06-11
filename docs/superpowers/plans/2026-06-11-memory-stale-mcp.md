# Memory Stale MCP Tool

## Goal

Expose the repository-level stale-memory review queue through the codel00p MCP
server so external agents can find approved memories superseded by newer active
project memory without shelling out to the CLI.

## Scope

- [x] Add a failing MCP stdio integration test for `memory_stale`.
- [x] Register `memory_stale` in `tools/list` with `kind`, `threshold`, and
  `limit` arguments.
- [x] Implement `tools/call` dispatch through `MemoryStalenessQuery`.
- [x] Return stable JSON text with stale memory fields, `score`, and nested
  `newer` memory record metadata.
- [x] Update CLI/memory/product docs to mark MCP stale queues as started.
- [x] Run focused MCP verification and full `pnpm verify`.
- [ ] Commit and push without coauthor trailers.
