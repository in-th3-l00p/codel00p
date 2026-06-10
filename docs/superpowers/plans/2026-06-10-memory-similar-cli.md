# Memory Similar CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose near-duplicate memory scoring through the CLI for review
workflows.

**Architecture:** Add `codel00p memory similar --content <content> --kind <kind>
[--threshold <score>] [--limit <n>] [--json]`. Default output is TSV with
`id`, `status`, `kind`, `score`, and `content`. JSON output is an array of
record objects plus `score`.

**Tech Stack:** Rust, `codel00p-cli`, `codel00p-memory::MemorySimilarityQuery`,
existing SQLite-backed CLI tests.

---

### Task 1: Add Red CLI Similar Test

**Files:**
- Modify: `core/crates/codel00p-cli/tests/memory_cli.rs`

- [x] **Step 1: Assert `memory similar` prints active near matches**

Create approved, unrelated, and archived similar memory records. Run:

```bash
codel00p memory similar --kind workflow --content "Run pnpm verify before pushing to main branch." --threshold 70
codel00p memory similar --kind workflow --content "Run pnpm verify before pushing to main branch." --threshold 70 --json
```

Assert only the active similar memory is returned with score `75`.

- [x] **Step 2: Run focused CLI memory test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_similar_scores_active_near_duplicates
```

Expected: FAIL because `memory similar` does not exist yet.

### Task 2: Implement CLI Similar Output

**Files:**
- Modify: `core/crates/codel00p-cli/src/memory.rs`

- [x] **Step 1: Add `similar` command dispatch and option parsing**

Parse required `--content` and `--kind`, optional `--threshold`, optional
`--limit`, and optional `--json`.

- [x] **Step 2: Serialize TSV and JSON similar results**

Default output should print stable TSV fields. JSON output should match CLI
memory record JSON plus `score`.

- [x] **Step 3: Run focused CLI memory test to verify pass**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_similar_scores_active_near_duplicates
```

Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-cli/README.md`
- Modify: `core/crates/codel00p-memory/README.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Document CLI similar scoring**

Mention `memory similar` in CLI and memory docs, and update Memory 2.0 notes to
include CLI review access to near-duplicate scores.

- [x] **Step 2: Run focused verification**

Run:

```bash
cd core
cargo fmt --all
cargo test -p codel00p-cli --test memory_cli
cargo test -p codel00p-cli --test help_cli
cargo clippy -p codel00p-cli --all-targets -- -D warnings
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
git add docs/superpowers/plans/2026-06-10-memory-similar-cli.md core/crates/codel00p-cli core/crates/codel00p-memory/README.md docs/agentic-backlog.md docs/product-roadmap.md
git commit -m "feat: add memory similar command"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
