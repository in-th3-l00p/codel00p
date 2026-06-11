# Memory Quality Review Query

## Goal

Add a deterministic core query for active memory records whose advisory quality
score is low enough to deserve human review.

## Scope

- [x] Add a failing memory lifecycle test for low-quality review selection.
- [x] Add `MemoryQualityQuery` and `QualityMemory`.
- [x] Add `MemoryRepository::quality_review(...)`.
- [x] Exclude rejected and archived memory from the active review queue.
- [x] Sort by quality score and memory id for deterministic review ordering.
- [x] Update memory docs and backlog notes.
