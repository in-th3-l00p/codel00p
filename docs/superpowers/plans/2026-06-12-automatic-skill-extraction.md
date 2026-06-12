# Automatic Skill Extraction

Second slice of [Initiative 3: Self-Improvement Loop](../../initiatives/self-improvement-loop.md)
(procedure synthesis). The agent now proposes skills *automatically* from
completed work — no explicit tool call required — still review-gated.

## Goal

After a turn that carried out a real procedure, propose a draft skill capturing
the goal and the steps, into the existing review queue.

## Scope

- [x] `codel00p-harness` `learning`: a `SkillExtractor` seam +
      `SkillExtractionRequest` (goal, final answer, executed tool names), and a
      deterministic `ProcedureSkillExtractor` that proposes one draft skill when
      the turn used >= `min_steps` mutating/command tool calls and ended with an
      answer (slug name from the goal, keyword triggers, the tool sequence as
      draft instructions). No extra inference call.
- [x] Harness wiring: `skill_extractor` + `skill_proposal_sink` on the builder;
      called at turn end after memory extraction, non-fatally (errors -> a
      `LifecycleHookFailed` event, never fails the turn).
- [x] CLI: the `learn` tool-set now also wires `ProcedureSkillExtractor` + a
      `CliSkillProposalSink`; the sink is **idempotent** (a duplicate or
      already-active name is a no-op), so repeated tasks stay quiet.
- [x] Tests: extractor unit tests (proposes on a real procedure; skips thin or
      unfinished turns) and two harness integration tests driving a full turn
      (a two-file-write procedure auto-proposes; a read-only turn proposes
      nothing).
- [x] `cargo test`, `cargo fmt --check`, `cargo clippy`.

## Decisions

- **Deterministic first.** A no-LLM extractor is predictable, free, and testable.
  It captures *what the agent did* (the tool sequence) as a draft for a human to
  refine — honest about being a skeleton. LLM-assisted synthesis can slot behind
  the same `SkillExtractor` seam later.
- **Conservative gate + idempotent sink.** Only multi-step, answered turns
  propose; the slug name dedups naturally (repeating a task re-proposes the same
  name, which the sink no-ops). This keeps the review queue from filling with
  noise.
- **Both paths share one queue.** Explicit `propose_skill` and automatic
  extraction land in the same `.candidates/` review queue, reviewed the same way.

## Out of scope (next slices)

- LLM-assisted extraction (a synthesis call) behind the same seam.
- Usage signal (record skill hit counts) — Phase 1.
- Curator (retire stale agent skills on a schedule) — Phase 2, needs Scheduling.
- A configurable `min_steps` (currently the sensible default of 2).
