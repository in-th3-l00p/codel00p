# Memory Stale CLI

**Goal:** Expose the repository stale-memory query through the CLI as a stable review queue.

**Architecture:** Add `codel00p memory stale` alongside `memory similar` and `memory search`. The command should build a `MemoryStalenessQuery` for the configured project, support optional `--kind`, `--threshold`, `--limit`, and `--json`, and print stable TSV by default. JSON output should reuse the memory record JSON shape and include `score` plus a nested `newer` record object.

**Tech Stack:** Rust, `codel00p-cli`, `codel00p-memory`, CLI integration tests, Markdown docs, `pnpm verify`.

## Test Plan

- [x] **Step 1: Test stale memory CLI review queue**

Add a CLI test that seeds:

- an approved original workflow memory;
- an unrelated active memory;
- an archived similar newer memory;
- a newer active candidate with similar content.

Run:

```bash
codel00p memory stale --kind workflow --threshold 70
codel00p memory stale --kind workflow --threshold 70 --json
```

Assert TSV output includes `id`, `status`, `kind`, `score`, `newer_id`, and content, and JSON output includes `score` and a nested `newer` record object with source evidence.

- [ ] **Red Command**

```bash
cd core && cargo test -p codel00p-cli --test memory_cli memory_stale_lists_superseded_approved_memory
```

Expected: failure because `memory stale` is not implemented yet.

## Implementation Plan

- [x] **Step 1: Add CLI command parsing**

Modify: `core/crates/codel00p-cli/src/memory.rs`

Dispatch `memory stale`, parse `--kind`, `--threshold`, `--limit`, and `--json`, and reject unknown options with `unknown memory stale option`.

- [x] **Step 2: Add TSV and JSON output**

Default TSV fields:

```text
id	status	kind	score	newer_id	content
```

JSON output should extend stale record JSON with:

- `score`;
- `newer`, using the same memory record JSON helper.

- [x] **Step 3: Update help text**

Add `stale` to `codel00p memory --help`.

## Documentation Plan

- [x] **Step 1: Update CLI README**

Document `memory stale` commands and stable output.

- [x] **Step 2: Update memory README and roadmap**

Mark stale-memory detection as CLI-visible, with MCP exposure left for later.

## Verification Plan

- [x] **Focused verification**

```bash
cd core && cargo fmt --all && cargo test -p codel00p-cli --test memory_cli memory_stale_lists_superseded_approved_memory && cargo test -p codel00p-cli --test memory_cli && cargo test -p codel00p-cli && cargo clippy -p codel00p-cli --all-targets -- -D warnings
```

- [x] **Full repo verification**

```bash
pnpm verify
```

## Commit Plan

- [x] Commit as `feat: add memory stale command` and push to `origin/main`.
