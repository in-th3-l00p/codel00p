# Memory Show JSON CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a single memory record scriptable from the CLI with the same
shape MCP already returns.

**Architecture:** Add an optional `--json` flag to `codel00p memory show <id>`.
The existing text output remains unchanged. JSON output should include `id`,
`status`, `kind`, `content`, `tags`, and optional `source.session_id` /
`source.turn_id`.

**Tech Stack:** Rust, `codel00p-cli`, serde JSON, existing SQLite-backed CLI
tests.

---

### Task 1: Add Red CLI Show JSON Test

**Files:**
- Modify: `core/crates/codel00p-cli/tests/memory_cli.rs`

- [x] **Step 1: Assert `memory show --json` prints stable record JSON**

In `memory_show_and_audit_print_stable_details`, run:

```bash
codel00p memory show mem-workflow --json
```

Parse stdout as JSON and assert `id`, `status`, `kind`, `content`, `tags`, and
source evidence.

- [x] **Step 2: Run focused CLI memory test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_show_and_audit_print_stable_details
```

Expected: FAIL because `memory show` does not accept `--json` yet.

### Task 2: Implement Show JSON Output

**Files:**
- Modify: `core/crates/codel00p-cli/src/memory.rs`

- [x] **Step 1: Parse optional `--json` flag**

Allow `memory show <id> --json` and reject unknown show options.

- [x] **Step 2: Serialize memory record objects**

When `--json` is present, output a JSON object matching MCP memory record JSON.
Keep the existing text output unchanged when the flag is absent.

- [x] **Step 3: Run focused CLI memory test to verify pass**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_show_and_audit_print_stable_details
```

Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-cli/README.md`
- Modify: `core/crates/codel00p-memory/README.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Document CLI show JSON mode**

Mention `memory show <id> --json` in CLI and memory docs, and update Memory
2.0 notes from CLI-visible source evidence to CLI JSON source evidence.

- [x] **Step 2: Run focused verification**

Run:

```bash
cd core
cargo fmt --all
cargo test -p codel00p-cli --test memory_cli
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
git add docs/superpowers/plans/2026-06-10-memory-show-json-cli.md core/crates/codel00p-cli core/crates/codel00p-memory/README.md docs/agentic-backlog.md docs/product-roadmap.md
git commit -m "feat: add memory show json output"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
