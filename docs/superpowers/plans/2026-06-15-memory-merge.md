# Slice: Memory merge (Memory 2.0)

Date: 2026-06-15
Roadmap: Milestone 3 (Memory 2.0) — merge/split workflows.
Backlog: Active Priority #2 (Memory 2.0 — "the product moat").

## Goal

Let a reviewer fold a duplicate memory into a canonical one in a single,
audited operation: archive the duplicate, carry its tags over to the survivor,
and leave a two-sided audit trail linking them. This is the natural next step
after the existing near-duplicate (`similar`) and stale detection: detection
already surfaces the duplicates; merge is the action that resolves them.

## Why this slice

It is the smallest merge/split increment that ships independently and is fully
offline-testable: it reuses the existing repository/audit plumbing, needs no new
storage primitive, and turns the read-only `memory similar` finding into a
one-command cleanup. Chosen for isolation from the in-flight TUI/cloud
org-members work (different crate, different files).

## Design

A merge has a **source** (the duplicate, archived away) and a **target** (the
survivor, kept and enriched).

1. **Core audit** (`codel00p-memory::review`): add
   `MemoryAuditAction::Merged` and a `MemoryAuditEvent::merged{,_from}` carrying
   an optional `merged_into` reference. The source's event records
   `merged_into = Some(target)`; the target's event records `merged_into = None`
   with the source id in `reason` ("absorbed <source>"), so each memory's audit
   log explains its side of the merge.
2. **Merge value object** (`MemoryMerge { actor, reason }`) mirroring
   `ReviewDecision`/`MemoryEdit`.
3. **Repository** (`store`): `merge(source_id, target_id, MemoryMerge)`:
   - reject merging a memory into itself;
   - require both memories in the **same project** and both **active**
     (candidate or approved);
   - union the source's tags into the target (order-preserving, deduped) and
     persist the enriched target;
   - archive the source (a merge-specific transition, distinct from review's
     approved→archived rule);
   - append the two audit events; return the updated **target** record.
4. **Error**: `MemoryError::InvalidMerge { message }` for the rejection cases.
5. **CLI** (`memory merge <source> <target> --actor A [--reason R] [--json]`):
   dispatch + formatter; `merged` audit label; `merged_into` in audit JSON.

## Tests (test-first)

- core lifecycle: merge unions tags, archives the source, keeps the target
  active, and writes both audit events with the right `merged_into` direction;
- core guards: self-merge, cross-project, inactive source, inactive target all
  return `InvalidMerge` and mutate nothing;
- CLI: `memory merge` archives the source and prints the target; `--json`
  surfaces the enriched target; the source's `audit --json` shows `merged` +
  `merged_into`.

## Out of scope

- MCP `memory merge` tool (the audit *serializer* already renders `merged`;
  the write tool is the immediate follow-up slice).
- Split (the inverse) and interactive merge from `memory similar`/the TUI.
