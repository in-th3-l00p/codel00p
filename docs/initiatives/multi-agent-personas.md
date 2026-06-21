# Initiative 13: Persistent Multi-Agent Personas

Make codel00p host **multiple persistent agents**, each a named entity with its
own persona, configuration, and **growing memory** that learns from
conversations and persists across sessions — creatable and switchable from the
menu, with a default agent started on launch. Modeled on the Hermes Agent
(NousResearch) multi-agent design, adapted to codel00p's architecture and
governance.

## The key insight (why this is cheap)

Hermes achieves "multiple agents" with **one runtime and N homes**, isolated
entirely by the `HERMES_HOME` env var: a profile is a directory holding its own
config, memory, sessions, and skills. **codel00p already has this exact
primitive** — `CODEL00P_HOME` (`settings/paths.rs:home_dir()`) scopes the config
(`config.toml`), the memory DB (`default_memory_db()`), and sessions. So:

> **An agent = a `CODEL00P_HOME`-scoped directory.** Switching agents = pointing
> `CODEL00P_HOME` at it. Per-agent memory, sessions, and config come for free
> from the home boundary — no `agent_id` dimension threaded through storage.

This is the decisive design choice (Option A below). It is faithful to Hermes and
reuses machinery codel00p already ships.

## What an agent is (data model)

```
~/.codel00p/agents/<name>/          # this dir is the agent's CODEL00P_HOME
├── config.toml                     # layered settings + a [profile] (reuses #12 profiles)
├── persona.md                      # durable persona → system-prompt "who am I" slot 1
├── memory.sqlite                   # the agent's OWN growing memory (BM25 + proactive recall)
├── sessions/ (+ session store)     # the agent's OWN conversations
├── skills/                         # the agent's OWN learned procedural memory
└── agent.toml                      # metadata: name, description (for routing), created_at, distribution
```
The base `~/.codel00p` is the **default agent**, always present, used when no
agent is selected (back-compatible: today's single-home behavior == the default
agent).

Persona has two layers (Hermes' SOUL.md vs `/personality`): **`persona.md`** is
the durable identity injected first each turn; lightweight **session personality
overlays** reuse the existing profile/personality idea for transient voice.

## Reuse map (what already exists)

| Need | Already in codel00p |
|---|---|
| Home isolation primitive | `CODEL00P_HOME` / `home_dir()` (`settings/paths.rs`) |
| Agent entity + persona field | Cloud `agents.rs` + `protocol/cloud.rs` Agent (id/name/description/**instructions**/provider/model/mcp) — currently cloud-only |
| Behavior config bundle | `[agent.profiles.<name>]` + presets (#12) |
| Growing memory | `codel00p-memory` BM25 + **proactive task-aware recall** + shingle dedup + reviewed lifecycle (#107) |
| Learns from conversations | post-session memory recommender + skill/capability extractors (`harness/memory.rs`, `learning.rs`) |
| Persona/system-prompt injection | `instructions.rs` + base prompt + `self_context.rs` (self block already carries name/profile) |
| Sessions + switcher | `codel00p-session` + TUI `SessionSwitcher` |
| Menu to create/switch | TUI Entity Browser **already has an "Agents" tab** (cloud picker) + Ctrl+P palette |

## Design options

- **Option A — Home-per-agent (RECOMMENDED, Hermes-faithful).** Each agent is its
  own `CODEL00P_HOME`. Memory/sessions/config isolate automatically. Minimal
  plumbing; maximal isolation; matches Hermes exactly. Sharing (org knowledge) is
  opt-in later via distributions / a shared memory scope.
- **Option B — `agent_id` dimension.** Add `agent_id` to `StorageScope` +
  `SessionMetadata` + memory records; one shared home, partitioned by agent. More
  invasive (threads `agent_id` everywhere) but lets agents share a project's store
  with partitioning. Kept as the fallback if home-per-agent proves limiting.

The plan below assumes **Option A**.

## Phased plan

### Phase 1 — Local agent registry + lifecycle (CLI)
- [ ] A **local agent model + registry**: agents live under `~/.codel00p/agents/<name>/`; list by scanning that dir; an `agent.toml` per agent (name, description, created_at). Reuse the cloud Agent field shape so cloud/local converge.
- [ ] `codel00p agent create <name> [--clone | --clone-from <a>] [--description ..] [--model ..] [--persona-file ..]` — scaffold the home (config seeded from defaults/a profile, `persona.md`, fresh memory/sessions). `--clone` copies config/persona/skills but **fresh memory+sessions** (Hermes semantics).
- [ ] `codel00p agent use <name>` (sticky default), `--agent <name>` / per-command, `agent list`, `agent show`, `agent rename`, `agent delete`. Switching resolves the agent's home as `CODEL00P_HOME`.
- [ ] **Persona injection**: load `<agent home>/persona.md` and inject it as the first system block (ahead of base prompt / instructions), and set the self-context `name` to the agent name.
- [ ] **Default agent on launch**: with no `--agent`/sticky selection, use the base home (the default agent) — exactly today's behavior, so this is back-compatible.

### Phase 2 — TUI menu (create / switch / default-on-open)
- [x] A dedicated **agent switcher overlay** wired to the local registry (lists `default` + every `<base>/agents/<name>`, marks active, **live-switches** on select). Reachable via Ctrl+P → "Switch agent" and `/agent`. (The Entity Browser "Agents" tab stays the cloud org-agent picker.)
- [x] Ctrl+P palette: `CommandAction::SwitchAgent` and `CommandAction::CreateAgent` (a small name + description form; richer creation stays in `agent create`).
- [x] Load the **active/default agent at TUI startup** (`event_loop.rs`/`app.rs`), show it in the header banner (`agent: <name>` / `agent: default`), and scope the session switcher to the active agent (sessions resolve from the active home's `memory.sqlite`).
- [x] **Live memory switch**: selecting an agent re-points the running TUI's `CODEL00P_HOME` + rebuilds `App.config` so the next harness build uses the new agent's `memory.sqlite` and sessions, resetting to a fresh session under it (delivered in #13 phase 3).

### Phase 3 — Per-agent growing memory + learning (mostly free under Option A)
- [ ] Confirm the memory recommender + skill/capability extractors write into the **active agent's home** (they already write to `CODEL00P_HOME`'s memory DB / skills dir), so each agent learns independently. Add tests proving isolation (agent A's memories/skills don't appear for agent B).
- [ ] Optional **capped curated memory** à la Hermes MEMORY.md/USER.md: a small always-injected per-agent notes file + a user-profile file (token-capped), distinct from the searchable memory DB. (Differentiator; keeps a cheap "always-known" layer.)
- [ ] A **curator** pass (codel00p already has staleness/quality scoring): add opt-in consolidation of near-duplicate learned skills/memories per agent (shingle similarity from #107), archive-not-delete, behind review — fights skill/memory sprawl.

### Phase 4 — Sharing, orchestration, advanced memory (later)
- [ ] **Agent distributions**: package an agent (persona + config + skills) as a shareable artifact (git), **excluding keys + memory/sessions** (Hermes' hard exclusion). `agent install <ref>` / `export`.
- [ ] **Routing/orchestration**: use each agent's `description` to route delegated tasks to the right specialist agent (pairs with sub-agent fan-out in #12 / kanban).
- [ ] **Pluggable external memory provider seam** (Honcho-style running user-model) behind the `MemoryRanker`/provider seam from #107 — optional, additive, governance-gated (sends content out).

## Customizability & governance fit

- An agent **composes over profiles** (#12): its `config.toml` can set `agent.profile` and any `[agent.behavior]` toggle, so "a careful Docker reviewer agent" vs "an autonomous local coder agent" is just two agent homes with different profiles.
- All existing governance holds **per agent**: reviewed memory/skills, permission scopes, audit, the verify-before-done loop, isolation policy — each agent's home carries its own reviewed knowledge.
- Default-on, opt-out: with no agents created, codel00p behaves exactly as today (the base home is the implicit default agent).

## Verified-vs-marketing (Hermes)

Relied upon (verified in the Hermes repo/docs): env-var home isolation; SOUL.md
persona slot; tiered memory (capped curated files + unbounded FTS5 history +
procedural skills); agent-written skills with a background curator; git-repo
distributions excluding user data; per-agent `description` for routing. Treated as
marketing (not relied upon): "self-improving AI" framing (it's prompt/file/skill
accretion + heuristics, not weight updates) and the "FTS5 + LLM summarization"
phrasing (FTS5 recall is verbatim; summarization is a separate compression path).

## Critical files (insertion points)

- `core/crates/codel00p-cli/src/settings/paths.rs` (`home_dir()`) — agent-home resolution.
- New: `core/crates/codel00p-cli/src/agent/registry.rs` (local agent model + list/create/use/delete) + an `agent` subcommand.
- `core/crates/codel00p-harness/src/{instructions.rs, self_context.rs}` — persona injection + agent name.
- `core/crates/codel00p-cli/src/tui/{overlay.rs, update.rs, view.rs, app.rs, event_loop.rs}` — Agents tab/create/switch + default-on-launch.
- `core/crates/codel00p-memory/*`, `harness/{memory.rs, learning.rs}` — confirm per-home scoping; per-agent isolation tests.
- Cloud `agents.rs` / `protocol/cloud.rs` — converge the local model with the existing cloud Agent shape.
