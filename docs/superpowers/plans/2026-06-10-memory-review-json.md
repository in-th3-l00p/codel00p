# Memory Review JSON Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add machine-readable JSON output to CLI memory lifecycle review
commands.

**Architecture:** Keep existing TSV output as the default. Add `--json` to the
shared `memory approve`, `memory reject`, and `memory archive` parser and return
the same record JSON shape used by `memory show --json`.

**Tech Stack:** Rust, `codel00p-cli`, memory CLI integration tests.

---

### Task 1: Add Red Review JSON Test

**Files:**
- Modify: `core/crates/codel00p-cli/tests/memory_cli.rs`

- [x] **Step 1: Assert approve/archive support `--json`**

Extend the review lifecycle test so `memory approve ... --json` and
`memory archive ... --json` return JSON records with `id`, `status`, `kind`,
`content`, `tags`, `source`, and `source_uri`.

- [x] **Step 2: Run focused test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_review_commands_persist_state_across_invocations
```

Expected: FAIL because review commands reject `--json` today.

### Task 2: Implement Review JSON Output

**Files:**
- Modify: `core/crates/codel00p-cli/src/memory.rs`

- [x] **Step 1: Parse `--json` in shared review command**

Accept `--json` for approve, reject, and archive.

- [x] **Step 2: Return record JSON when requested**

Serialize `memory_record_json(&record)` when `--json` is present; preserve TSV
output when omitted.

- [x] **Step 3: Run focused test to verify pass**

Run the same focused test from Task 1.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-cli/README.md`
- Modify: `core/crates/codel00p-cli/src/help.rs`

- [x] **Step 1: Document review `--json`**

Mention JSON output for approve/reject/archive in CLI help and README.

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
git add docs/superpowers/plans/2026-06-10-memory-review-json.md core/crates/codel00p-cli
git commit -m "feat: add memory review json output"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
