# Memory Quality Status Filter

## Goal

Let reviewers split low-quality memory cleanup queues by active review status.

## Checklist

- [x] Add failing core memory coverage for `MemoryQualityQuery` status filtering.
- [x] Add failing CLI and MCP coverage for status-filtered quality review.
- [x] Add `MemoryQualityQuery::with_status` and filter active memory by status.
- [x] Wire CLI `memory quality --status candidate|approved` and MCP `memory_quality.status`.
- [x] Update memory docs, CLI README, and active backlog notes.
- [x] Run focused checks and full repository verification.
