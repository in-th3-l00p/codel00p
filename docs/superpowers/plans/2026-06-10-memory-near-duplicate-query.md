# Memory Near-Duplicate Query Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add deterministic near-duplicate detection as a memory-domain
foundation without changing candidate creation semantics.

**Architecture:** Add `MemorySimilarityQuery` and `SimilarMemory`, plus a
`MemoryRepository::similar_active` method. The query compares proposed content
against active candidate/approved memory in the same project and kind. Scoring
uses normalized token Jaccard similarity as an integer percentage, sorted by
score descending and memory id ascending.

**Tech Stack:** Rust, `codel00p-memory`, existing in-memory lifecycle tests,
existing storage-backed repository.

---

### Task 1: Add Red Near-Duplicate Test

**Files:**
- Modify: `core/crates/codel00p-memory/tests/lifecycle.rs`

- [x] **Step 1: Assert similar active memory is returned with score**

Create an approved workflow memory, an unrelated workflow memory, and an
archived similar memory. Query similar active memory for:

```text
Run pnpm verify before pushing to main branch.
```

Assert only the active similar memory is returned with score `75`.

- [x] **Step 2: Run focused memory test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-memory --test lifecycle finds_near_duplicate_active_memory
```

Expected: FAIL because the similarity query API does not exist yet.

### Task 2: Implement Similarity Query

**Files:**
- Modify: `core/crates/codel00p-memory/src/lib.rs`

- [x] **Step 1: Add query/result types and trait method**

Add `MemorySimilarityQuery`, `SimilarMemory`, and
`MemoryRepository::similar_active`.

- [x] **Step 2: Implement deterministic token similarity**

Normalize alphanumeric tokens to lowercase, compute integer Jaccard percentage,
filter by `min_score`, ignore non-active records, and sort by score descending
then id ascending.

- [x] **Step 3: Run focused memory test to verify pass**

Run:

```bash
cd core
cargo test -p codel00p-memory --test lifecycle finds_near_duplicate_active_memory
```

Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-memory/README.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Document near-duplicate query foundation**

Mention the new repository query in memory docs, and update Memory 2.0 notes
from exact duplicate rejection only to include near-duplicate scoring.

- [x] **Step 2: Run focused verification**

Run:

```bash
cd core
cargo fmt --all
cargo test -p codel00p-memory --test lifecycle
cargo clippy -p codel00p-memory --all-targets -- -D warnings
```

Expected: all commands pass.

- [x] **Step 3: Run repo verification**

Run:

```bash
pnpm verify
```

Expected: PASS.

- [x] **Step 4: Commit and push**

Run:

```bash
git add docs/superpowers/plans/2026-06-10-memory-near-duplicate-query.md core/crates/codel00p-memory docs/agentic-backlog.md docs/product-roadmap.md
git commit -m "feat: add memory near duplicate query"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
