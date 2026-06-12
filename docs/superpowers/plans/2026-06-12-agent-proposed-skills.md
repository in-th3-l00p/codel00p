# Agent-Proposed Skills (Self-Improvement Loop)

First slice of [Initiative 3: Self-Improvement Loop](../../initiatives/self-improvement-loop.md)
(procedure synthesis, Phase 3). Closes the core "grows with you" loop:
**agent does work → proposes a skill → human reviews → on approval it becomes
active and is auto-applied on future turns.**

## Goal

Let an agent capture a reusable procedure it learned as a *review candidate*
(never auto-applied), and give humans a CLI to approve/reject it. Approved skills
flow into the existing skill-injection machinery.

## Scope

- [x] `codel00p-skill` candidate lifecycle: `SkillProposal`, `propose_skill`
      (writes `<root>/.candidates/<name>/SKILL.md`, dedup vs active + existing
      candidates, name validation), `load_candidates`, `approve_candidate`
      (activates), `reject_candidate` (archives under `.candidates/.archive`),
      plus a `created_by` provenance field on `Skill`.
- [x] `codel00p-harness` `learning` module: `ProposedSkill`, a
      `SkillProposalSink` seam, a permission-scoped `propose_skill` tool, and
      `learning_tools(sink)`. The harness stays decoupled from skill storage.
- [x] CLI: a `learn` tool-set registers `propose_skill` backed by
      `CliSkillProposalSink` (writes to the user skills dir, `created_by: agent`);
      `codel00p skills candidates | approve | reject`.
- [x] Tests: skill-crate candidate lifecycle (propose/dedup/approve/reject/unsafe
      names), harness tool, and an end-to-end `agent_cli` test where the model
      calls `propose_skill`, the candidate appears (and is *not* active), then
      `skills approve` activates it.
- [x] Help + `configuration.md` document the `learn` tool-set and review queue.
- [x] `cargo test` (skill + harness + cli single-threaded), `cargo fmt --check`,
      `cargo clippy`.

## Why this is the heart of the loop

codel00p already had the *governance* half (review lifecycle) and, from the
Skills initiative, *injection* of approved skills. This slice adds the
*generation* half — and critically keeps it **review-gated**: proposals live
under `.candidates/` and are never loaded as active skills, so an unreviewed
machine proposal can never reach a future turn's context. That is codel00p's
deliberate divergence from Hermes (whose agent skills go live immediately): the
loop proposes, humans approve.

## Decisions

- **Candidates as files, not a new store.** Proposals are `SKILL.md` files under
  `.candidates/`, reusing the loader/parser; transparent, git-friendly, and
  inactive by construction. Approve = move into the active set; reject = archive.
- **`created_by` provenance.** Recorded in front matter and surfaced on `Skill`,
  so the future curator can target `agent`-authored skills without touching
  human-approved or bundled ones.
- **Explicit proposal first.** The agent proposes via a tool call (opt-in `learn`
  tool-set). Fully automatic post-task extraction is the next slice; doing it
  explicitly first keeps the signal high and the behavior auditable.

## Out of scope (next slices)

- Automatic post-task skill extraction (no explicit tool call).
- Usage signal (record skill injection/hit counts) — Phase 1.
- Curator: retire stale `created_by: agent` skills on a schedule — Phase 2
  (needs Scheduling, initiative #5).
- Project/user modeling and cross-session recall — Phase 4.
