# Memory Source Evidence CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show existing memory source evidence in `codel00p memory show` output.

**Architecture:** Reuse the existing `MemoryEntry::source()` protocol field and keep storage/schema unchanged. Add source session and source turn lines to the detail view when a memory has source metadata, so reviewers can trace candidates back to the session turn that produced them.

**Tech Stack:** Rust CLI, `codel00p-memory`, `codel00p-protocol`, SQLite-backed CLI tests.

---

### Task 1: Add Red CLI Source Evidence Assertion

**Files:**
- Modify: `core/crates/codel00p-cli/tests/memory_cli.rs`

- [x] **Step 1: Update memory show expected output**

In `memory_show_and_audit_print_stable_details`, update the expected `show`
stdout to include source lines:

```rust
assert_eq!(
    stdout(&show),
    "id: mem-workflow\nstatus: candidate\nkind: workflow\ntags: verify\nsource_session: session-cli\nsource_turn: turn-cli\ncontent: Run pnpm verify before pushing main.\n"
);
```

- [x] **Step 2: Run the focused CLI test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_show_and_audit_print_stable_details
```

Expected: FAIL because `memory show` does not print source evidence yet.

### Task 2: Implement Source Evidence Output

**Files:**
- Modify: `core/crates/codel00p-cli/src/memory.rs`

- [x] **Step 1: Build detail output line-by-line**

Change `memory_show` from a single `format!` call to a mutable `String`:

```rust
let mut output = format!(
    "id: {}\nstatus: {}\nkind: {}\ntags: {}\n",
    record.entry().id(),
    status_label(record.entry().status()),
    kind_label(record.entry().kind()),
    record.entry().tags().join(","),
);
```

- [x] **Step 2: Add optional source lines**

Append source lines before `content`:

```rust
if let Some(source) = record.entry().source() {
    output.push_str(&format!(
        "source_session: {}\nsource_turn: {}\n",
        source.session_id().as_str(),
        source.turn_id().as_str()
    ));
}
```

- [x] **Step 3: Add content line and return output**

Append:

```rust
output.push_str(&format!("content: {}\n", record.entry().content()));
Ok(output)
```

- [x] **Step 4: Run the focused CLI test to verify pass**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_show_and_audit_print_stable_details
```

Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-memory/README.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Document visible source evidence**

Mention that memory candidates keep source session/turn metadata and `memory
show` exposes it for review.

- [x] **Step 2: Run focused verification**

Run:

```bash
cd core
cargo fmt --all
cargo test -p codel00p-cli --test memory_cli
cargo test -p codel00p-memory
cargo test -p codel00p-memory --features sqlite
cargo clippy -p codel00p-cli --all-targets -- -D warnings
cargo clippy -p codel00p-memory --all-targets -- -D warnings
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
git add docs/superpowers/plans/2026-06-10-memory-source-evidence-cli.md core/crates/codel00p-cli core/crates/codel00p-memory docs
git commit -m "feat: show memory source evidence"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
