# Memory Stale Query

**Goal:** Add a core repository query that flags approved memories likely superseded by newer active memory in the same project and kind.

**Architecture:** Keep stale detection deterministic and storage-backed. Add `MemoryStalenessQuery` and `StaleMemory` to `codel00p-memory`, plus `MemoryRepository::stale_active`. The query should compare approved memories against newer active candidate/approved records in index creation order using the existing token-overlap score. It should ignore archived/rejected newer records and return at most one highest-scoring newer match per stale memory.

**Tech Stack:** Rust, `codel00p-memory`, lifecycle tests, Markdown docs, `pnpm verify`.

## Test Plan

- [x] **Step 1: Test stale approved memory detection**

Add a lifecycle test that:

- creates and approves an original workflow memory;
- creates an unrelated active memory;
- creates and archives a similar newer memory;
- creates a similar newer active candidate;
- runs `stale_active(MemoryStalenessQuery::new(project()).with_kind(MemoryKind::Workflow).with_min_score(70))`;
- asserts the original approved memory is returned with the newer active candidate as the superseding record and score `75`.

- [ ] **Red Command**

```bash
cd core && cargo test -p codel00p-memory --test lifecycle detects_stale_approved_memory_superseded_by_newer_active_memory
```

Expected: compile failure because `MemoryStalenessQuery` and `MemoryRepository::stale_active` do not exist yet.

## Implementation Plan

- [x] **Step 1: Add query and result types**

Modify: `core/crates/codel00p-memory/src/lib.rs`

Add:

- `MemoryStalenessQuery` with project, optional kind, minimum score, and optional limit;
- `StaleMemory` with stale record, newer record, and score accessors.

- [x] **Step 2: Preserve index sequence for stale comparisons**

Add an internal indexed record helper based on the existing memory audit index stream. Use its append sequence as deterministic creation order.

- [x] **Step 3: Implement stale detection**

Add `MemoryRepository::stale_active`. For each approved memory matching the query, find newer active candidate/approved memories in the same project and kind with token overlap at or above the minimum score. Return the highest-scoring newer match per stale memory, sorted deterministically by score descending and then memory id.

## Documentation Plan

- [x] **Step 1: Update memory README**

Document stale-memory detection as a core repository query, with CLI/MCP exposure left for follow-up.

- [x] **Step 2: Update Memory 2.0 status docs**

Mark stale-memory detection as started with repository-level newer-overlap scoring.

## Verification Plan

- [x] **Focused verification**

```bash
cd core && cargo fmt --all && cargo test -p codel00p-memory --test lifecycle detects_stale_approved_memory_superseded_by_newer_active_memory && cargo test -p codel00p-memory --test lifecycle && cargo test -p codel00p-memory && cargo clippy -p codel00p-memory --all-targets -- -D warnings
```

- [x] **Full repo verification**

```bash
pnpm verify
```

## Commit Plan

- [x] Commit as `feat: add memory stale query` and push to `origin/main`.
