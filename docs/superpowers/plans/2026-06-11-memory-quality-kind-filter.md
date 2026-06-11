# Memory Quality Kind Filter

## Goal

Let reviewers narrow low-quality memory cleanup queues to one memory kind.

## Checklist

- [x] Add failing core memory coverage for `MemoryQualityQuery` kind filtering.
- [x] Add failing CLI and MCP coverage for kind-filtered quality review.
- [x] Add `MemoryQualityQuery::with_kind` and filter active memory by kind.
- [x] Wire CLI `memory quality --kind <kind>` and MCP `memory_quality.kind`.
- [x] Update memory docs, CLI README, and active backlog notes.
- [x] Run focused checks and full repository verification.
