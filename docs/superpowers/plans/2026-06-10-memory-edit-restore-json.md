# Memory Edit Restore JSON Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add machine-readable JSON output to CLI memory edit and restore
commands.

**Architecture:** Keep existing TSV output as the default. Add `--json` to
`memory edit` and `memory restore`, returning the same record JSON shape used by
`memory show --json` and MCP write tools.

**Tech Stack:** Rust, `codel00p-cli`, memory CLI and help integration tests.

---

### Task 1: Add Red Edit/Restore JSON Tests

**Files:**
- Modify: `core/crates/codel00p-cli/tests/memory_cli.rs`

- [x] **Step 1: Assert edit supports `--json`**

Extend the edit test so `memory edit ... --json` returns a JSON record with the
updated content and source fields.

- [x] **Step 2: Assert restore supports `--json`**

Extend the restore test so `memory restore ... --json` returns a JSON record
with the restored content and source fields.

- [x] **Step 3: Run focused tests to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_edit_updates_content_and_prints_audit_event
cargo test -p codel00p-cli --test memory_cli memory_restore_reverts_to_previous_edit_content
```

Expected: FAIL because edit and restore reject `--json` today.

### Task 2: Implement Edit/Restore JSON Output

**Files:**
- Modify: `core/crates/codel00p-cli/src/memory.rs`

- [x] **Step 1: Parse `--json` for edit**

Accept `--json` in `memory edit` without changing required arguments.

- [x] **Step 2: Parse `--json` for restore**

Accept `--json` in `memory restore` without changing required arguments.

- [x] **Step 3: Return record JSON when requested**

Serialize `memory_record_json(&record)` when `--json` is present; preserve TSV
output when omitted.

- [x] **Step 4: Run focused tests to verify pass**

Run the same focused tests from Task 1.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-cli/README.md`
- Modify: `core/crates/codel00p-cli/src/help.rs`
- Modify: `core/crates/codel00p-cli/tests/help_cli.rs`

- [x] **Step 1: Document edit/restore `--json`**

Mention JSON output for `memory edit` and `memory restore` in CLI help and
README.

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
git add docs/superpowers/plans/2026-06-10-memory-edit-restore-json.md core/crates/codel00p-cli
git commit -m "feat: add memory edit restore json output"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
