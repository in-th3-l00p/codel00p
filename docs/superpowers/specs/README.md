# Design specs

Longer-form design documents that precede or span several
[slice plans](../plans/README.md). Where a slice plan answers "what is the next
shippable step," a spec answers "what is the shape of this subsystem."

Convention: `YYYY-MM-DD-<subsystem>-design.md`. Cross-link a spec from the slice
plan(s) that implement it so neither is a dead-end.

Current specs:

- [`2026-06-07-agent-harness-design.md`](2026-06-07-agent-harness-design.md) —
  the original `codel00p-harness` runtime design (turn loop, tool dispatch,
  provider boundary, event hooks). Implemented across the `codel00p-harness`
  crate; see [`../../harness.md`](../../harness.md) for the current reference.
