# Memory Sensitivity Metadata

## Goal

Add a narrow sensitivity contract for project memory so sensitive approved
knowledge can be stored and reviewed without being injected into ordinary
inference context.

## Scope

- [x] Add red protocol, repository, CLI, and MCP tests for sensitivity.
- [x] Add `MemorySensitivity` to the protocol with `normal` default and
  `sensitive` opt-in.
- [x] Persist sensitivity on memory candidates and preserve it through review
  and edits.
- [x] Exclude sensitive approved memory from default retrieval, while allowing
  explicit sensitivity queries.
- [x] Expose sensitivity in CLI/MCP JSON and add CLI/MCP sensitivity filters.
- [x] Update memory docs, backlog, roadmap, and help text.
- [x] Run focused and full verification.
- [x] Commit and push without coauthor trailers.
