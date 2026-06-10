# Memory Exact Duplicate Candidate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent exact duplicate active memory candidates from accumulating.

**Architecture:** Add a repository-level exact duplicate check during candidate creation. A new candidate is a duplicate when an existing memory in the same project has the same kind, the same trimmed content, and active status (`candidate` or `approved`). Rejected and archived memories do not block replacement candidates. Agent candidate sinks should treat this duplicate error the same way they already treat duplicate IDs: skip it and report it as a duplicate instead of failing the turn.

**Tech Stack:** Rust, `codel00p-memory`, `codel00p-harness`, `codel00p-cli`, in-memory and existing SQLite-backed storage paths.

---

### Task 1: Add Red Core Duplicate Test

**Files:**
- Modify: `core/crates/codel00p-memory/tests/lifecycle.rs`

- [x] **Step 1: Assert exact active duplicate content is rejected**

Add a test that creates one workflow candidate and then tries to create a
second workflow candidate in the same project with a different id but the same
trimmed content. Expect a duplicate error that points to the existing memory id,
and assert only one memory remains in the list.

- [x] **Step 2: Run the focused lifecycle test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-memory --test lifecycle rejects_exact_duplicate_active_memory_content
```

Expected: FAIL because duplicate content is not detected yet.

### Task 2: Implement Active Exact Duplicate Detection

**Files:**
- Modify: `core/crates/codel00p-memory/src/lib.rs`
- Modify: `core/crates/codel00p-harness/src/memory.rs`
- Modify: `core/crates/codel00p-cli/src/agent.rs`

- [x] **Step 1: Add duplicate memory error variant**

Add a `DuplicateMemory { id, existing_id }` error variant to `MemoryError`.

- [x] **Step 2: Check for active duplicate before persisting**

In `StorageBackedMemoryStore::create_candidate`, after id uniqueness
validation and before `put_record`, scan existing records for active duplicate
project/kind/trimmed-content matches. Return `DuplicateMemory` with the new id
and the existing id when found.

- [x] **Step 3: Treat duplicate memory as skipped in sinks**

Update `MemoryRepositoryCandidateSink` and `CliMemoryCandidateSink` to add the
new candidate id to `duplicate_ids` when `DuplicateMemory` occurs.

- [x] **Step 4: Run the focused lifecycle test to verify pass**

Run:

```bash
cd core
cargo test -p codel00p-memory --test lifecycle rejects_exact_duplicate_active_memory_content
```

Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-memory/README.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Document exact duplicate detection**

Document the exact active duplicate rule and update Memory 2.0 foundation notes
to mark duplicate detection as started.

- [x] **Step 2: Run focused verification**

Run:

```bash
cd core
cargo fmt --all
cargo test -p codel00p-memory --test lifecycle
cargo test -p codel00p-harness --test agent_turn duplicate_extracted_candidates_are_skipped_without_auto_approval
cargo test -p codel00p-cli --test agent_cli agent_run_extracts_reviewed_memory_and_reuses_approved_memory
cargo clippy -p codel00p-memory --all-targets -- -D warnings
cargo clippy -p codel00p-harness --all-targets -- -D warnings
cargo clippy -p codel00p-cli --all-targets -- -D warnings
```

Expected: all commands pass.

- [x] **Step 3: Run repo verification**

Run:

```bash
pnpm verify
```

Expected: PASS.

- [ ] **Step 4: Commit and push**

Run:

```bash
git add docs/superpowers/plans/2026-06-10-memory-exact-duplicate-candidates.md core/crates/codel00p-memory core/crates/codel00p-harness/src/memory.rs core/crates/codel00p-cli/src/agent.rs docs/agentic-backlog.md docs/product-roadmap.md
git commit -m "feat: reject duplicate active memory"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
