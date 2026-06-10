# Memory Edit Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a core memory edit primitive that updates content and records an audit event.

**Architecture:** Extend `codel00p-memory` with a `MemoryEdit` value and `MemoryRepository::edit`. Editing replaces content only, preserves status/source/tags, and appends an `edited` audit event with actor and optional reason. CLI editing is intentionally left for a later slice, but CLI audit labels should understand the new event.

**Tech Stack:** Rust, `codel00p-memory`, `codel00p-cli` audit label, storage-backed memory repository tests.

---

### Task 1: Add Red Memory Edit Test

**Files:**
- Modify: `core/crates/codel00p-memory/tests/lifecycle.rs`

- [x] **Step 1: Import `MemoryEdit`**

Update the test import list to include `MemoryEdit`.

- [x] **Step 2: Add edit lifecycle test**

Add this test after `lifecycle_changes_are_audited_in_order`:

```rust
#[test]
fn memory_edit_updates_content_and_audits_revision() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-1",
            project(),
            MemoryKind::Workflow,
            "Run tests before pushing.",
            source(),
        ))
        .expect("create candidate");
    store
        .review("mem-1", ReviewDecision::approve("alice"))
        .expect("approve candidate");

    let edited = store
        .edit(
            "mem-1",
            MemoryEdit::replace_content("bob", "Run pnpm verify before pushing main.")
                .with_reason("clarified verification command"),
        )
        .expect("edit memory");
    let audit = store.audit_log("mem-1").expect("audit log");

    assert_eq!(edited.entry().status(), MemoryStatus::Approved);
    assert_eq!(
        edited.entry().content(),
        "Run pnpm verify before pushing main."
    );
    assert_eq!(edited.entry().source(), Some(&source()));
    assert_eq!(audit.len(), 3);
    assert_eq!(audit[2].sequence(), 3);
    assert_eq!(audit[2].action(), MemoryAuditAction::Edited);
    assert_eq!(audit[2].actor(), "bob");
    assert_eq!(audit[2].reason(), Some("clarified verification command"));
}
```

- [x] **Step 3: Run the focused lifecycle test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-memory --test lifecycle memory_edit_updates_content_and_audits_revision
```

Expected: FAIL because `MemoryEdit`, `MemoryRepository::edit`, and
`MemoryAuditAction::Edited` do not exist yet.

### Task 2: Implement Memory Edit Audit

**Files:**
- Modify: `core/crates/codel00p-memory/src/lib.rs`
- Modify: `core/crates/codel00p-cli/src/memory.rs`

- [x] **Step 1: Add `MemoryEdit`**

Add:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryEdit {
    actor: String,
    content: String,
    reason: Option<String>,
}
```

Implement `replace_content`, `with_reason`, `actor`, `content`, `reason`, and
`validate` methods. Reject empty content with `MemoryError::InvalidEdit`.

- [x] **Step 2: Add audit action**

Add `Edited` to `MemoryAuditAction`, and add `MemoryAuditEvent::edited`.

- [x] **Step 3: Extend `MemoryError` and `MemoryRepository`**

Add:

```rust
InvalidEdit { message: String },
```

to `MemoryError`, and add:

```rust
fn edit(&mut self, id: &str, edit: MemoryEdit) -> Result<MemoryRecord, MemoryError>;
```

to `MemoryRepository`.

- [x] **Step 4: Implement repository edit**

In `StorageBackedMemoryStore`, load the current record, validate the edit,
rebuild a `MemoryEntry` with the new content and existing status/source/tags,
store it, and append an edited audit event.

- [x] **Step 5: Update CLI audit action label**

In `core/crates/codel00p-cli/src/memory.rs`, map
`MemoryAuditAction::Edited` to `"edited"`.

- [x] **Step 6: Run the focused lifecycle test to verify pass**

Run:

```bash
cd core
cargo test -p codel00p-memory --test lifecycle memory_edit_updates_content_and_audits_revision
```

Expected: PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `core/crates/codel00p-memory/README.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Document memory edit audit**

Mention that the core memory repository can edit content while preserving
metadata and appending an `edited` audit event.

- [x] **Step 2: Run memory verification**

Run:

```bash
cd core
cargo fmt --all
cargo test -p codel00p-memory
cargo test -p codel00p-memory --features sqlite
cargo test -p codel00p-cli --test memory_cli
cargo clippy -p codel00p-memory --all-targets -- -D warnings
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
git add docs/superpowers/plans/2026-06-10-memory-edit-audit.md core/crates/codel00p-memory core/crates/codel00p-cli docs
git commit -m "feat: add memory edit audit"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
