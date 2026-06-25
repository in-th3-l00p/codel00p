# Plan: Per-Agent Curator Pass (multi-agent personas #13, Phase 3 final box)

**Goal.** Opt-in, per-agent consolidation of near-duplicate learned **memories** and
**skills**. Detection reuses the existing shingle/Jaccard similarity (no LLM cost).
All actions are **propose-for-review** and **archive-not-delete** (reversible),
mirroring the existing skills/memory approve-reject UX. Fights knowledge sprawl as
an agent accumulates memories/skills over many sessions.

## Decisions locked
- **Detection:** reuse `shingle_similarity` (deterministic, local, no LLM).
- **Apply mode:** propose-for-review — consolidations land as candidates a human
  approves/rejects; originals are archived, never deleted.
- **Consolidation semantics (no-LLM consequence):** for a near-dup cluster, keep a
  **canonical survivor** and propose archiving the rest as superseded. No new merged
  text is synthesized. Survivor selection:
  - **Memory:** highest `score_memory_entry` quality; tie → most recently updated.
  - **Skills:** highest usage `count`; tie → most recently created.
- **Toggle default:** `agent.behavior.curator` defaults **OFF** (the plan says
  "opt-in"; unlike `persona`/`curated_memory`/`proactive_memory` which default on).

## What already exists (reuse, don't rebuild)
- `codel00p-memory/src/ranking.rs:238` `shingle_similarity(left,right)->u8` (0..=100,
  token bigrams). Tests: reworded dup ≥40, unrelated 0.
- `codel00p-memory` `MemoryRepository` (repository.rs:13): `similar_active(query)`,
  `stale_active(query)`, `quality_review(query)`, `review(id, decision)`, `list(filter)`.
- `codel00p-memory/src/review.rs:10` `ReviewDecision::Archive{actor, reason}` →
  `MemoryStatus::Archived`. Archive is already a first-class, reasoned transition.
- `codel00p-protocol/src/memory.rs:7` `MemoryStatus{Candidate,Approved,Rejected,Archived}`.
- `codel00p-memory/src/records.rs:167` `score_memory_entry()->MemoryQuality` (0..=100).
- `codel00p-skill/src/lib.rs`: `is_curatable` (321), `archive_skill`/`restore_skill`
  (282–319), `Skill.created_by` provenance (91), `SkillSource`.
- `codel00p-skill/src/usage.rs:22` `SkillUsage{count,last_used_epoch}`, `load_usage`.
- CLI review UX to mirror: `codel00p-cli/src/skills.rs` (`candidates`/`approve`/`reject`/
  `curate` at 39–48, `skills_curate` 322–371) and `codel00p-cli/src/memory.rs`
  (`approve`/`reject`/`archive` 40–54). TUI dialogs: `skills_ui/mod.rs`, `memory_ui/`.
- Behavior schema: `codel00p-cli/src/settings/schema.rs:319` `BehaviorSettings`
  (default-on getters ~441–528).

## Gaps to build
1. **Skill↔skill similarity.** `shingle_similarity` lives in `codel00p-memory` and is
   `pub(crate)`. Skills have no cross-skill near-dup detection. Need a shared,
   dependency-light similarity fn usable by both crates.
2. **Cluster + survivor selection** over `similar_active` results (memory) and skill
   bodies (skills) — pure logic, no store changes.
3. **Curator proposal surface** — a dry-run report + `--apply` that enacts archives
   through the *existing* review/archive APIs, plus a config toggle gate.

## Phased implementation

### Phase A — shared similarity seam (small, enabling) — DONE
- [x] Extracted `tokenize` + `shingle_similarity` (with `shingles`/`SHINGLE_N`/`STOPWORDS`)
  into a new dependency-light leaf crate **`codel00p-textsim`** (pure std). `codel00p-memory`
  depends on it and re-exports both via `ranking::{tokenize, shingle_similarity}`, so all
  existing in-crate call sites (`store.rs`) are untouched. `codel00p-skill` can depend on
  textsim directly in Phase C (clean dep direction, no skill→memory edge).
- [x] Ported the two shingle tests to `codel00p-textsim` (+ identical-is-100 and a tokenize
  test); BM25 tests stay in `ranking.rs`. Memory behavior is byte-identical.

### Phase B — memory curator (detection already exists) — DONE
- [x] `curator` module in `codel00p-memory`: `pub fn plan_consolidations(records, threshold)
  -> Vec<Consolidation>` — pure/deterministic. Groups by kind, union-finds near-dup clusters
  via `shingle_similarity`, and per cluster picks a **survivor** = highest
  `MemoryQuality::score` (ties → smallest id) with the rest as `DuplicateMemory{ record,
  similarity }`. No store mutation; callers apply separately. Unit-tested (cluster, quality
  survivor, different-kinds-never-merge, below-threshold, determinism).
- [x] `codel00p memory curate` CLI (`memory/curate.rs`): lists approved active memories,
  plans, and renders a dry-run (keep/archive lines + `≥threshold%` summary). `--apply` calls
  `store.review(dup_id, ReviewDecision::archive("…", "curator: near-duplicate of <id> (<n>% similar)"))`
  on each duplicate — audited + reversible via the existing review/restore path. `--json`,
  `--threshold`, `--actor` flags. Integration-tested (dry-run, json, apply+isolation, re-run no-op).
- [x] Threshold constant `DEFAULT_CONSOLIDATION_THRESHOLD: u8 = 60` (overridable via
  `--threshold`); conservative since `--apply` archives.

> **Quality note (verified):** `score_memory_entry` counts **unique tokens including
> stopwords** (`records::content_tokens`), distinct from ranking's stopword-dropping
> `tokenize`. So survivor selection rewards the memory with more distinct content. Kept as-is.

### Phase C — skill curator
- [ ] `fn skill_consolidations(skills, usage, threshold) -> Vec<SkillConsolidation>`:
  pairwise shingle similarity over skill instruction bodies, **agent-authored only**
  (`created_by == "agent"`, same guard as `is_curatable`), cluster, survivor = most-used.
- [ ] Extend `codel00p-cli/src/skills.rs` `skills_curate`: today it archives *stale unused*
  agent skills; add a **near-duplicate** section (dry-run + `--apply` → `archive_skill`).
  Keep the two concerns clearly separated in output.

### Phase D — gate, TUI, docs
- [ ] Add `agent.behavior.curator: Option<bool>` to `BehaviorSettings` (default **false**;
  getter `.unwrap_or(false)`). CLI `memory curate` / `skills curate` near-dup pass refuse
  to run (or warn) when disabled, matching how other behaviors gate.
- [ ] (Optional, later) surface curator proposals in `memory_ui`/`skills_ui` review dialogs
  as a "consolidate" action, reusing the existing mutation/apply closures.
- [ ] Docs: update `docs/initiatives/multi-agent-personas.md` Phase 3 box to `[x]`, note
  the no-LLM keep-best semantics; add a short "agent curation" section to user docs.

## Isolation / governance fit
- Curator operates entirely within the active agent's `CODEL00P_HOME` (memory.sqlite +
  home/skills), so it is **per-agent by construction** — no new isolation work.
- Everything is reviewed + reversible (archive-not-delete), consistent with the existing
  reviewed-knowledge governance. Survivor choice is deterministic and explainable
  (quality score / usage count + similarity %), so proposals are auditable.

## Test plan
- Unit: similarity seam round-trips memory's existing tests; clustering picks correct
  survivor; threshold boundaries; agent-only skill guard.
- Integration (`codel00p-cli`): seed an agent home with near-dup memories+skills; dry-run
  lists clusters; `--apply` archives non-survivors; survivors + originals recoverable;
  disabled toggle ⇒ no-op.
- E2E (`codel00p-e2e/multi_agent.rs` style): two agents, curate one, assert the other's
  memories/skills untouched (isolation), and archived items don't surface in recall.

## Sequencing note
Phase B (memory) is the cheapest win — detection (`similar_active`) and archive
(`ReviewDecision::Archive`) both already exist, so it's a CLI + clustering layer only.
Recommend landing A+B first as a reviewable PR, then C, then D.
