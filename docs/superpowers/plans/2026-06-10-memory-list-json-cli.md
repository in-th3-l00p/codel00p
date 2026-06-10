# Memory List JSON CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make filtered memory browsing scriptable from the CLI with the same
record shape used by `memory show --json` and MCP memory JSON.

**Architecture:** Add an optional `--json` flag to `codel00p memory list`. The
existing TSV output remains unchanged. JSON output should be an array of memory
record objects with `id`, `status`, `kind`, `content`, `tags`, and optional
`source.session_id` / `source.turn_id`.

**Tech Stack:** Rust, `codel00p-cli`, serde JSON, existing SQLite-backed CLI
tests.

---

### Task 1: Add Red CLI List JSON Test

**Files:**
- Modify: `core/crates/codel00p-cli/tests/memory_cli.rs`

- [x] **Step 1: Assert `memory list --json` prints stable record JSON**

In `memory_list_prints_filtered_candidates`, run:

```bash
codel00p memory list --status candidate --kind workflow --tag verify --json
```

Parse stdout as JSON and assert the filtered record array includes stable
fields and source evidence.

- [x] **Step 2: Run focused CLI memory test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_list_prints_filtered_candidates
```

Expected: FAIL because `memory list` does not accept `--json` yet.

### Task 2: Implement List JSON Output

**Files:**
- Modify: `core/crates/codel00p-cli/src/memory.rs`

- [x] **Step 1: Parse optional `--json` flag**

Allow `memory list --json` alongside existing filters and reject unknown list
options.

- [x] **Step 2: Serialize memory record arrays**

When `--json` is present, output an array of JSON objects matching
`memory show --json`. Keep the existing TSV output unchanged when the flag is
absent.

- [x] **Step 3: Run focused CLI memory test to verify pass**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_list_prints_filtered_candidates
```

Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-cli/README.md`
- Modify: `core/crates/codel00p-memory/README.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Document CLI list JSON mode**

Mention `memory list --json` in CLI and memory docs, and update Memory 2.0
notes from CLI show JSON source evidence to CLI list/show JSON source evidence.

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
git add docs/superpowers/plans/2026-06-10-memory-list-json-cli.md core/crates/codel00p-cli core/crates/codel00p-memory/README.md docs/agentic-backlog.md docs/product-roadmap.md
git commit -m "feat: add memory list json output"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
