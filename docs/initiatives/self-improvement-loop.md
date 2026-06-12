# Initiative 3: Self-Improvement Loop

## Goal

Close the loop so codel00p gets more capable the more a team uses it: it
proposes new procedures and memory from completed work, tracks what actually
gets used, retires what goes stale, and builds a durable model of the project and
its users — all inside the existing review/audit governance.

## Why (Hermes reference)

Hermes's defining property is "an autonomous agent that gets more capable the
longer it runs," via a closed learning loop:

- **Autonomous skill creation** from user interactions.
- **Self-improving skills** that refine themselves during use.
- **Curator** (`agent/curator.py`) tracking usage in `~/.hermes/skills/.usage.json`,
  auto-archiving stale `created_by: agent` skills after `stale_after_days`, with
  pre-run tar.gz snapshots for rollback. Bundled/hub skills are off-limits.
- **Agent-curated memory** with periodic persistence nudges.
- **Cross-session recall** via FTS5 full-text search + LLM summarization.
- **User modeling** via Honcho's dialectic approach.

## Current codel00p state

codel00p has the *governance half* of this loop already, which Hermes lacks:

- `ExplicitTurnMemoryExtractor` produces memory candidates at turn end (capped,
  default 8).
- Candidate -> review -> approve/reject -> archive lifecycle with full audit
  history (`memory audit`, `memory approve/reject/archive/restore`).
- Quality scoring (`MemoryRecord::quality()`), staleness scoring
  (`memory stale`), similarity/near-duplicate detection (`memory similar`).

What is missing is the *improvement half*: usage feedback, procedure generation,
automated staleness retirement, and a persistent user/project model.

## Design

Build on top of [#2 Skills](skills-system.md) and `codel00p-memory`. Everything
stays inside the review lifecycle — the loop **proposes**, humans (or org
policy) **approve**.

### 1. Usage feedback signal
- Record retrieval and application events: when a memory entry or skill is
  injected into a turn and when it demonstrably contributed (tool used, edit
  made). Persist through `codel00p-storage` as an append log.
- Surface in `memory quality` / `skills list`: usage count, last-used, hit rate.

### 2. Curator
- A scheduled job (uses [#5 Scheduling](scheduling-cron.md)) that:
  - flags `created_by: agent` skills/memory unused past a threshold as
    **archive candidates** (never silently deletes — codel00p divergence),
  - reconciles staleness scores already produced by `memory stale`,
  - keeps bundled, hub-installed, and human-approved entries off-limits.
- Snapshots before any mutation (reuse SQLite/storage versioning) for rollback,
  matching Hermes's tar.gz snapshot behavior.

### 3. Procedure generation (skill synthesis)
- After a successful multi-step task, an extractor proposes a `SKILL.md`
  candidate capturing the reusable procedure (the commands/edits/sequence),
  alongside the declarative memory candidates already extracted.
- Proposed skills enter as **review candidates**, not live procedures.

### 4. Project & user modeling
- A durable, append-only "project model" and "user model" document in
  `codel00p-storage`, refined across sessions (the Honcho-dialectic analogue),
  scoped per `project_id` / per `user_id`.
- For teams: user models are **per-member and org-visible by policy**, not a
  single personal profile — fits the control-plane positioning.
- Sensitivity scopes already in `codel00p-memory` apply (normal/sensitive) so
  the model never leaks confidential context into shared retrieval.

### 5. Cross-session recall
- codel00p already retrieves approved memory deterministically; add an optional
  FTS/summarization recall pass over prior session transcripts
  (`codel00p-session`) for long-horizon continuity, behind the same
  deterministic filters.

## Scope

### Phase 1 — Usage signal
- [x] Skill usage tracking: a `.usage.json` per skills root (count + last-used),
      recorded once per turn when a skill is injected, surfaced in `skills list`.
      Slice:
      [2026-06-12-skill-usage-tracking](../superpowers/plans/2026-06-12-skill-usage-tracking.md).
      This is the signal the curator consumes.
- [ ] Memory retrieval/application usage signal surfaced in `memory quality`.

### Phase 2 — Curator
- [ ] Scheduled curator job producing archive candidates for stale agent-created
      entries (review-gated, snapshot-backed, never silent).
- [ ] Reconcile with existing staleness/similarity scoring.

### Phase 3 — Procedure synthesis
- [x] Agent-proposed skills: a `propose_skill` tool (the `learn` tool-set) lets
      the agent capture a learned procedure as a review candidate; CLI
      `skills candidates/approve/reject` close the loop, and approved skills are
      auto-injected on future turns (the complete proposes→review→apply loop).
      Slice:
      [2026-06-12-agent-proposed-skills](../superpowers/plans/2026-06-12-agent-proposed-skills.md).
- [x] Automatic post-task extraction: a `SkillExtractor` seam + a deterministic
      `ProcedureSkillExtractor` proposes a draft skill when a turn carried out a
      real procedure (>= N mutating/command tool calls) and answered, feeding the
      same review queue (idempotent, no extra inference). Slice:
      [2026-06-12-automatic-skill-extraction](../superpowers/plans/2026-06-12-automatic-skill-extraction.md).
- [ ] LLM-assisted extraction (synthesize higher-quality skills) behind the same
      `SkillExtractor` seam.

### Phase 4 — Project/user model
- [ ] Per-project and per-user model documents, refined across sessions, with
      sensitivity scoping and org-visibility policy.
- [ ] Optional cross-session FTS recall over `codel00p-session`.

## Risks & open questions

- **Auto-apply vs review**: Hermes lets agent skills go live immediately;
  codel00p must keep human/policy review or it breaks the "only reviewed
  knowledge reaches context" thesis. Loop proposes, never auto-applies.
- **Feedback attribution**: "did this memory help?" is noisy; start with coarse
  retrieved/used counters before anything fancier.
- **Privacy**: user modeling in a team product needs explicit consent and
  sensitivity scoping; default to project-scoped, opt-in for user-scoped.

## Dependencies

- [#2 Skills](skills-system.md) (procedure store + lifecycle).
- [#5 Scheduling](scheduling-cron.md) (curator runs on a schedule).
- Existing `codel00p-memory` scoring/audit and `codel00p-session`.

## Exit criteria

- After a team uses codel00p across many sessions, it measurably surfaces better
  procedures and memory (tracked by usage hit rate), retires stale agent-created
  knowledge automatically into a review queue, and maintains a project/user
  model that improves retrieval — all auditable and reversible.
