# Memory Quality Tag Filter

## Goal

Let reviewers narrow low-quality memory cleanup queues by tag.

## Checklist

- [x] Add failing core memory coverage for `MemoryQualityQuery` tag filtering.
- [x] Add failing CLI and MCP coverage for tag-filtered quality review.
- [x] Add `MemoryQualityQuery::with_tag` and filter active memory by tag.
- [x] Wire CLI `memory quality --tag <tag>` and MCP `memory_quality.tag`.
- [x] Update memory docs, CLI README, and active backlog notes.
- [x] Run focused checks and full repository verification.
