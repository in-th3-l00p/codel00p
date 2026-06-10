# Memory Audit JSON CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make memory audit history, including edit revision metadata, scriptable from the CLI.

**Architecture:** Add an optional `--json` flag to `codel00p memory audit <id>`. The existing TSV output remains unchanged. JSON output should be an array of audit event objects with `sequence`, `action`, `actor`, optional `reason`, and optional `previous_content`/`new_content` fields.

**Tech Stack:** Rust, `codel00p-cli`, serde JSON, existing SQLite-backed CLI tests.

---

### Task 1: Add Red CLI Audit JSON Test

**Files:**
- Modify: `core/crates/codel00p-cli/tests/memory_cli.rs`

- [x] **Step 1: Assert `memory audit --json` prints revision metadata**

In `memory_edit_updates_content_and_prints_audit_event`, run:

```bash
codel00p memory audit mem-workflow --json
```

Parse stdout as JSON and assert the edit event contains `previous_content` and
`new_content`.

- [x] **Step 2: Run focused CLI memory test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_edit_updates_content_and_prints_audit_event
```

Expected: FAIL because `memory audit` does not accept `--json` yet.

### Task 2: Implement Audit JSON Output

**Files:**
- Modify: `core/crates/codel00p-cli/src/memory.rs`

- [x] **Step 1: Parse optional `--json` flag**

Allow `memory audit <id> --json` and reject unknown audit options.

- [x] **Step 2: Serialize audit event objects**

When `--json` is present, output a JSON array with stable labels and optional
fields. Keep the existing TSV output unchanged when the flag is absent.

- [x] **Step 3: Run focused CLI memory test to verify pass**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_edit_updates_content_and_prints_audit_event
```

Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-cli/README.md`
- Modify: `core/crates/codel00p-memory/README.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Document CLI audit JSON mode**

Mention `memory audit <id> --json` in CLI and memory docs, and update Memory
2.0 notes from MCP-visible revision metadata to CLI/MCP-visible metadata.

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
git add docs/superpowers/plans/2026-06-10-memory-audit-json-cli.md core/crates/codel00p-cli core/crates/codel00p-memory/README.md docs/agentic-backlog.md docs/product-roadmap.md
git commit -m "feat: add memory audit json output"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
