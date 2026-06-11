# Memory Quality Score

## Goal

Give review workflows a deterministic first-pass quality score for individual
memory records before adding CLI, MCP, or cloud review queues.

## Scope

- [x] Add a failing memory lifecycle test for low-value memory scoring.
- [x] Add `MemoryRecord::quality()` and `MemoryQuality` with score and findings.
- [x] Keep scoring deterministic and local to memory entry shape.
- [x] Update memory docs and backlog notes.
