# Skill Context Injection

Second slice of [Initiative 2: Skills System](../../initiatives/skills-system.md),
completing Phase 1. Makes skills actually affect agent runs.

## Goal

Select skills relevant to the user's request and inject them into the turn's
inference context, the same way reviewed project memory is injected.

## Scope

- [x] `codel00p-skill`: `select_skills(skills, query, limit)` — deterministic
      relevance by case-insensitive trigger/name substring match; unrelated
      queries select nothing.
- [x] `codel00p-harness` `skills` module: generic `SkillPrompt` / `SkillContext`
      (a name + instructions; no dependency on the skill crate), a
      `SkillProvider` seam, and a `SkillPromptAssembler` that renders a system
      prompt. `HarnessInferenceRequest::with_skills`; `provider_adapter` adds the
      assembled prompt as a system message after project memory.
- [x] Harness run loop calls the optional `skill_provider` each turn with the
      latest user message as the relevance query.
- [x] CLI `CliSkillProvider` loads layered skills and selects up to 3 relevant
      ones; wired into `build_agent_harness` (so `agent run` and `agent chat`
      both get skills).
- [x] Tests: skill-crate relevance selection; harness assembler; and an
      end-to-end `agent_cli` test proving a trigger-matched skill's body reaches
      the provider request.
- [x] `cargo test` (harness + skill + cli single-threaded), `cargo fmt --check`,
      `cargo clippy`.

## Decisions

- **Mirror project memory exactly.** Skills inject through the same request ->
  provider-adapter path as reviewed memory, as a distinct system message. The
  harness stays decoupled from skill storage by seeing only `SkillPrompt`s.
- **Relevance is deterministic and conservative.** Substring trigger matching is
  predictable and testable; an empty or unrelated query injects nothing, so
  skills never bloat context when irrelevant. Semantic ranking can come later
  behind the same `SkillProvider` seam.
- **Selected inline, not persisted.** Selection reloads skills from disk per
  turn (like the memory provider); fine for local skill counts, optimizable
  later.

## Phase 1 is complete

Skills can be authored (`skills create`), inspected (`skills list/show`), and now
**apply automatically** when relevant to a request. Later phases: a `use_skill`
tool + permission-scoped skill scripts, usage tracking, agent-authored skills in
the review lifecycle, and hub install / agentskills.io import.
