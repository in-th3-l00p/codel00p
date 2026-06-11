# Memory Quality JSON

## Goal

Expose memory quality scores in machine-readable CLI and MCP memory records so
review surfaces can prioritize cleanup without a new command.

## Scope

- [x] Add failing CLI and MCP JSON assertions for memory quality.
- [x] Add quality accessors to retrieved, similar, and stale memory wrappers.
- [x] Include `quality.score` and `quality.findings` in CLI memory JSON.
- [x] Include the same quality object in MCP memory JSON.
- [x] Update memory CLI, MCP, and backlog docs.
