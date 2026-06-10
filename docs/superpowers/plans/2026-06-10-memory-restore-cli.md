# Memory Restore CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a narrow revision restore workflow for memory edits.

**Architecture:** Add `codel00p memory restore <id> --sequence <n> --actor <actor>
[--reason <reason>]`. The command loads the audit event at `sequence`, requires
that it has `previous_content`, then restores that content through the existing
`MemoryRepository::edit` path. Restore therefore preserves id/status/kind/tags
and appends a normal `edited` audit event with revision metadata.

**Tech Stack:** Rust, `codel00p-cli`, existing memory audit events, existing
SQLite-backed CLI tests.

---

### Task 1: Add Red CLI Restore Test

**Files:**
- Modify: `core/crates/codel00p-cli/tests/memory_cli.rs`

- [x] **Step 1: Assert restore uses edit audit revision metadata**

Create approved memory, edit it, then run:

```bash
codel00p memory restore mem-workflow --sequence 3 --actor carol --reason "undo edit"
```

Assert the memory content returns to the edit event's `previous_content`, and
assert audit JSON includes a new `edited` event for the restore.

- [x] **Step 2: Run focused CLI memory test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_restore_reverts_to_previous_edit_content
```

Expected: FAIL because `memory restore` does not exist yet.

### Task 2: Implement CLI Memory Restore

**Files:**
- Modify: `core/crates/codel00p-cli/src/memory.rs`

- [x] **Step 1: Add `restore` command dispatch and option parsing**

Parse required `--sequence` and `--actor`, plus optional `--reason`.

- [x] **Step 2: Restore via existing edit path**

Find the audit event with the requested sequence, require `previous_content`,
and call `MemoryEdit::replace_content`. Default the reason when omitted to
`restore audit sequence <n>`.

- [x] **Step 3: Run focused CLI memory test to verify pass**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_restore_reverts_to_previous_edit_content
```

Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-cli/README.md`
- Modify: `core/crates/codel00p-memory/README.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Document CLI memory restore**

Mention `memory restore` in CLI and memory docs, and update Memory 2.0 notes to
include CLI restore over edit audit revisions.

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
git add docs/superpowers/plans/2026-06-10-memory-restore-cli.md core/crates/codel00p-cli core/crates/codel00p-memory/README.md docs/agentic-backlog.md docs/product-roadmap.md
git commit -m "feat: add memory restore command"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
