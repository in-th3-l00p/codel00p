# Memory Quality Sensitivity Filter

## Goal

Let reviewers isolate normal or sensitive low-quality memory in cleanup queues.

## Checklist

- [x] Add failing core memory coverage for `MemoryQualityQuery` sensitivity filtering.
- [x] Add failing CLI and MCP coverage for sensitivity-filtered quality review.
- [x] Add `MemoryQualityQuery::with_sensitivity` and filter active memory by sensitivity.
- [x] Wire CLI `memory quality --sensitivity normal|sensitive` and MCP `memory_quality.sensitivity`.
- [x] Update memory docs, CLI README, and active backlog notes.
- [x] Run focused checks and full repository verification.
