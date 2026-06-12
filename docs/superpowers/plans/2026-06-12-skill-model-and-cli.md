# Skill Model & CLI

First slice of [Initiative 2: Skills System](../../initiatives/skills-system.md),
Phase 1. Adds procedural memory codel00p can author, store, and inspect.

## Goal

Define the `Skill` model and `SKILL.md` format, a layered loader, and a
`codel00p skills` command to list, show, and scaffold skills.

## Scope

- [x] `codel00p-skill` crate: `Skill` (name, version, description, author,
      triggers, source, path, body), `SkillSource` (bundled < user < project),
      and a `SKILL.md` front-matter parser (scalars, block lists, inline lists,
      quotes; Markdown body).
- [x] `load_skills(sources)` — scans each source directory's `*/SKILL.md`,
      project overriding user overriding bundled by name; dependency-light
      (serde + thiserror only).
- [x] `codel00p skills list | show <name> | create <name> [--project]`, reading
      `~/.codel00p/skills` and the project `.codel00p/skills`.
- [x] Tests: crate unit tests (front-matter parse, inline lists + name
      fallback, missing-front-matter error, project-overrides-user) and a
      `skills_cli` integration suite (empty list, create/list/show round-trip,
      duplicate rejected, unknown show errors).
- [x] Help entry + blurb. `cargo test`, `cargo fmt --check`, `cargo clippy`.

## Decisions

- **TOML-everywhere, but `SKILL.md` uses `---` front matter.** Skills target the
  agentskills.io / Claude Code `SKILL.md` shape for portability, so the format
  is `---`-delimited front matter + Markdown. The parser handles the simple
  shape skills use in practice rather than pulling in a (currently unmaintained)
  general YAML dependency; richer YAML or a swap-in parser can come later.
- **Crate model now, protocol contract later.** Local skills only need the
  `codel00p-skill` model. A `SkillRecord` in `codel00p-protocol` (+ TS mirror)
  is deferred until cloud sync or the desktop surface needs it — same staging as
  other contracts.
- **Layered like config.** Project skills override user skills by name, matching
  how config, tool-sets, and plugins already resolve.

## Out of scope (next slices)

- Relevance selection (using `triggers`) + injecting selected skills into turn
  context (reuses memory retrieval).
- A `use_skill` tool via the plugin registry; permission-scoped skill scripts.
- Usage tracking; agent-authored skills entering the review lifecycle.
- Hub install (`skills install`) and agentskills.io import; org skill sync.
