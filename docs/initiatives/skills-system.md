# Initiative 2: Skills System

## Goal

Add **procedural memory**: reusable, named procedures the agent (or a human) can
author once and invoke later, extending codel00p's existing *declarative*
reviewed memory into *how-to* capability.

## Why (Hermes reference)

Skills are Hermes's headline mechanism for "growing with you":

- Skills are "procedural memory the agent creates and reuses," authored as a
  `SKILL.md` with frontmatter (name, version, author, dependencies).
- Built-in skills ship in `skills/`; heavier ones in `optional-skills/` install
  via `hermes skills install official/<category>/<skill>`.
- Open standard compatible with **agentskills.io** — portable, shareable,
  community-contributed via a Skills Hub.
- A **curator** tracks usage and auto-archives stale agent-created skills (see
  [#3](self-improvement-loop.md)).

codel00p's own harness already consumes Claude-Code-style skills at the tooling
level; this initiative makes codel00p itself *host* them.

## Current codel00p state

- `codel00p-memory` stores **declarative** knowledge (architecture, convention,
  workflow, decision, deployment, troubleshooting kinds) through a
  candidate -> review -> approve -> audit lifecycle, persisted in SQLite via
  `codel00p-storage`.
- Project instructions are loaded from `CODEL00P.md`, `AGENTS.md`, `CLAUDE.md`
  — a static, whole-file context injection, not invocable named procedures.
- There is no concept of a named, versioned, dependency-aware procedure the
  agent can load on demand.

## Design

### Skill format

Adopt the agentskills.io / `SKILL.md` shape for portability:

```
skill-name/
  SKILL.md        # frontmatter: name, version, author, description,
                  #   triggers, dependencies, permission scopes
  ...assets       # scripts, templates the skill references
```

- Frontmatter `description`/`triggers` drive **relevance selection** — load a
  skill into context only when the turn looks relevant, mirroring how codel00p
  already does deterministic memory retrieval (~8 most relevant entries).
- Declared `permission scopes` are enforced through the existing harness
  permission system; a skill cannot grant itself shell access silently.

### Storage & resolution

- Skill sources, in precedence order (mirrors layered config):
  `./.codel00p/skills/` (project) > `~/.codel00p/skills/` (user) > bundled.
- Index skills in `codel00p-memory` / `codel00p-storage` so relevance search and
  usage tracking reuse existing infrastructure rather than a parallel store.
- New `MemoryKind::Skill` or a sibling `SkillRecord` contract in
  `codel00p-protocol` (decision in Phase 1) so skills ride the same protocol,
  audit, and sync rails as memory.

### Invocation

- A skill is injected as procedural context when selected, OR exposed as a
  callable (a `use_skill` tool registered via the plugin registry from
  [#1](plugins-and-hooks.md)) so the model invokes it explicitly.
- Skills can reference scripts that run through the existing `run_command` tool
  under the skill's declared permission scope.

### CLI surface

- `codel00p skills list` — installed skills + source + usage.
- `codel00p skills show <name>` — render `SKILL.md`.
- `codel00p skills install <ref>` — from a hub (`official/<cat>/<skill>`) or path.
- `codel00p skills create <name>` — scaffold a `SKILL.md`.
- `codel00p skills archive/restore <name>` — lifecycle (curator-aligned).

### Governance fit (codel00p-specific)

- **Agent-authored skills enter as candidates**, not live procedures — they
  follow the same review lifecycle as memory candidates. This is the key
  divergence from Hermes, where agent skills are live immediately. It preserves
  codel00p's "only reviewed knowledge reaches future context" principle.
- The curator ([#3](self-improvement-loop.md)) only touches
  `created_by: agent` skills; bundled and hub-installed skills are off-limits
  (same rule as Hermes).
- `SOUL.md`-style personality/context is folded in here as a special bundled
  skill / context file, layered like config.

## Scope

### Phase 1 — Skill model & local skills
- [x] `Skill` model + `SKILL.md` front-matter parser in a `codel00p-skill`
      crate, with a layered loader (project > user > bundled). Slice:
      [2026-06-12-skill-model-and-cli](../superpowers/plans/2026-06-12-skill-model-and-cli.md).
      (A `SkillRecord` protocol contract + TS mirror is deferred to when cloud
      sync needs it; local skills use the crate model.)
- [x] `codel00p skills list/show/create`.
- [ ] Relevance selection reusing memory retrieval; inject selected skills into
      turn context.

### Phase 2 — Invocation & permissions
- [ ] `use_skill` tool via plugin registry; permission-scoped script execution.
- [ ] Usage tracking persisted through `codel00p-storage`.
- [ ] Agent-authored skills as review candidates (lifecycle reuse).

### Phase 3 — Hub & portability
- [ ] `codel00p skills install <ref>` from a hub; agentskills.io compatibility.
- [ ] Org-curated skill registry in `codel00p-cloud`; team skill sync.
- [ ] Export/import for sharing.

## Risks & open questions

- **Skill vs memory boundary**: keep one store and one protocol, or two? Leaning
  one store, `kind = skill`, to reuse retrieval/audit/sync.
- **Context cost**: skills are larger than memory entries; relevance selection
  and lazy loading matter for prompt size and cache stability.
- **Security**: a hub-installed skill can carry scripts; installs must surface
  declared permission scopes and require explicit approval.

## Dependencies

- [#1 Plugins & Hooks](plugins-and-hooks.md) for the `use_skill` tool seam.
- Builds directly on `codel00p-memory` retrieval/lifecycle/audit.

## Exit criteria

- A user or the agent can author a `SKILL.md`, have it stored under the existing
  review/audit lifecycle, and have it selected and applied on relevant later
  turns — with permission scopes enforced and usage tracked.
