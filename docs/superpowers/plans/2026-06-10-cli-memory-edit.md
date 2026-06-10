# CLI Memory Edit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `codel00p memory edit` command that edits memory content and records an `edited` audit event.

**Architecture:** Reuse the core `MemoryRepository::edit` primitive added in `codel00p-memory`. The CLI command parses a memory id plus `--actor`, `--content`, and optional `--reason`, writes through the existing SQLite-backed memory store, preserves the memory status/source/tags through the repository contract, and prints the same stable `id<TAB>status` output used by review commands.

**Tech Stack:** Rust, `codel00p-cli`, `codel00p-memory`, CLI integration tests, SQLite test storage.

---

### Task 1: Add Red CLI Memory Edit Test

**Files:**
- Modify: `core/crates/codel00p-cli/tests/memory_cli.rs`

- [x] **Step 1: Import `ReviewDecision` for setup**

Change the memory import to include `ReviewDecision`:

```rust
use codel00p_memory::{
    MemoryCandidateInput, MemoryListFilter, MemoryRepository, ReviewDecision,
    StorageBackedMemoryStore,
};
```

- [x] **Step 2: Add an approve helper**

Add this helper after `seed_candidate`:

```rust
fn approve_candidate(db_path: &Path, id: &str, actor: &str) {
    let storage = SqliteStorage::open(db_path).expect("open sqlite storage");
    let mut store =
        StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage);
    store
        .review(id, ReviewDecision::approve(actor))
        .expect("approve candidate");
}
```

- [x] **Step 3: Add the edit command test**

Add this test after `memory_review_commands_persist_state_across_invocations`:

```rust
#[test]
fn memory_edit_updates_content_and_prints_audit_event() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-workflow",
        MemoryKind::Workflow,
        "Run tests before pushing.",
        "verify",
    );
    approve_candidate(&db_path, "mem-workflow", "alice");

    let edit = run_codel00p(
        &db_path,
        &[
            "memory",
            "edit",
            "mem-workflow",
            "--actor",
            "bob",
            "--content",
            "Run pnpm verify before pushing main.",
            "--reason",
            "clarified command",
        ],
    );
    let show = run_codel00p(&db_path, &["memory", "show", "mem-workflow"]);
    let audit = run_codel00p(&db_path, &["memory", "audit", "mem-workflow"]);

    assert!(edit.status.success(), "stderr: {}", stderr(&edit));
    assert!(show.status.success(), "stderr: {}", stderr(&show));
    assert!(audit.status.success(), "stderr: {}", stderr(&audit));
    assert_eq!(stdout(&edit), "mem-workflow\tapproved\n");
    assert_eq!(
        stdout(&show),
        "id: mem-workflow\nstatus: approved\nkind: workflow\ntags: verify\nsource_session: session-cli\nsource_turn: turn-cli\ncontent: Run pnpm verify before pushing main.\n"
    );
    assert_eq!(
        stdout(&audit),
        "1\tcandidate_created\tsystem\t\n2\tapproved\talice\t\n3\tedited\tbob\tclarified command\n"
    );
}
```

- [x] **Step 4: Add the missing-content validation test**

Add this test after `memory_reject_requires_reason`:

```rust
#[test]
fn memory_edit_requires_content() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-workflow",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
    );

    let output = run_codel00p(
        &db_path,
        &["memory", "edit", "mem-workflow", "--actor", "alice"],
    );

    assert!(!output.status.success());
    assert!(stderr(&output).contains("missing required --content"));
}
```

- [x] **Step 5: Run the focused CLI test to verify failure**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_edit_updates_content_and_prints_audit_event
```

Expected: FAIL because `memory edit` is not a known memory command yet.

### Task 2: Implement CLI Memory Edit

**Files:**
- Modify: `core/crates/codel00p-cli/src/memory.rs`

- [x] **Step 1: Import `MemoryEdit`**

Change the import to:

```rust
use codel00p_memory::{MemoryEdit, MemoryListFilter, MemoryRepository, ReviewDecision};
```

- [x] **Step 2: Route the `edit` subcommand**

Add this match arm in `run`:

```rust
"edit" => memory_edit(config, rest),
```

Place it with the other mutating memory commands.

- [x] **Step 3: Add the edit parser and repository call**

Add this function after `memory_review`:

```rust
fn memory_edit(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some(id) = args.first() else {
        return Err("missing memory id".to_string());
    };
    let mut actor = None;
    let mut content = None;
    let mut reason = None;
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--actor" => {
                actor = Some(required_value(args, index, "--actor")?);
                index += 2;
            }
            "--content" => {
                content = Some(required_value(args, index, "--content")?);
                index += 2;
            }
            "--reason" => {
                reason = Some(required_value(args, index, "--reason")?);
                index += 2;
            }
            flag => return Err(format!("unknown memory edit option: {flag}")),
        }
    }

    let actor = actor.ok_or_else(|| "missing required --actor".to_string())?;
    let content = content.ok_or_else(|| "missing required --content".to_string())?;
    let mut edit = MemoryEdit::replace_content(actor, content);
    if let Some(reason) = reason {
        edit = edit.with_reason(reason);
    }

    let mut store = open_memory_store(&config)?;
    let record = store.edit(id, edit).map_err(|error| error.to_string())?;

    Ok(format!(
        "{}\t{}\n",
        record.entry().id(),
        status_label(record.entry().status())
    ))
}
```

- [x] **Step 4: Run the focused CLI test to verify pass**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_edit_updates_content_and_prints_audit_event
```

Expected: PASS.

- [x] **Step 5: Run the missing-content test**

Run:

```bash
cd core
cargo test -p codel00p-cli --test memory_cli memory_edit_requires_content
```

Expected: PASS.

### Task 3: Help, Docs, And Verification

**Files:**
- Modify: `core/crates/codel00p-cli/src/help.rs`
- Modify: `core/crates/codel00p-cli/tests/help_cli.rs`
- Modify: `core/crates/codel00p-memory/README.md`
- Modify: `docs/product-roadmap.md`
- Modify: `docs/agentic-backlog.md`

- [x] **Step 1: Add a failing help assertion**

In `core/crates/codel00p-cli/tests/help_cli.rs`, update the memory help assertion so it expects this line:

```rust
assert!(stdout(&memory_help).contains("edit     Edit memory content"));
```

- [x] **Step 2: Update memory help text**

In `core/crates/codel00p-cli/src/help.rs`, add the command line:

```text
  edit     Edit memory content
```

between `audit` and `approve`.

- [x] **Step 3: Document CLI edit availability**

In `core/crates/codel00p-memory/README.md`, update the edit audit section to mention `codel00p memory edit <id> --actor <actor> --content <content> [--reason <reason>]`.

In `docs/product-roadmap.md`, move CLI memory editing from next work into the current Memory 2.0 foundation.

In `docs/agentic-backlog.md`, update the Memory 2.0 edit bullet to say CLI editing is started.

- [x] **Step 4: Run focused verification**

Run:

```bash
cd core
cargo fmt --all
cargo test -p codel00p-cli --test memory_cli
cargo test -p codel00p-cli --test help_cli
cargo clippy -p codel00p-cli --all-targets -- -D warnings
```

Expected: all commands pass.

- [x] **Step 5: Run repo verification**

Run:

```bash
pnpm verify
```

Expected: PASS.

- [ ] **Step 6: Commit and push**

Run:

```bash
git add docs/superpowers/plans/2026-06-10-cli-memory-edit.md core/crates/codel00p-cli core/crates/codel00p-memory/README.md docs/agentic-backlog.md docs/product-roadmap.md
git commit -m "feat: add memory edit command"
git push origin main
```

Expected: commit has no coauthor trailers and pushes to `origin/main`.
