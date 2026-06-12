# Hermes Gap Analysis

A comparison of Hermes Agent (NousResearch) against codel00p, used to derive the
initiatives in this directory. The point is not to clone Hermes — codel00p is a
Rust, team/enterprise-governance platform where Hermes is a Python,
single-user-personal one — but to identify capability categories Hermes ships
that codel00p has no equivalent for.

References:
[Hermes docs](https://hermes-agent.nousresearch.com/docs/),
[hermes-agent repo](https://github.com/nousresearch/hermes-agent),
[AGENTS.md architecture](https://github.com/NousResearch/hermes-agent/blob/main/AGENTS.md).

## Centers of gravity

- **codel00p** — Rust workspace; layered TOML config; 9 providers + policy
  presets; Clerk Orgs/JWT cloud backend; reviewed/audited team memory; stable
  protocol contracts across CLI/desktop/cloud; MCP client + server; built-in
  read/edit/command/git tools with a permission system.
- **Hermes** — Python; one agent core (`AIAgent` in `run_agent.py`) shared
  across CLI, TUI, Electron, and a 20+ platform messaging gateway; designed to
  "get more capable the longer it runs" via a closed learning loop.

## Gap table

| Capability | Hermes | codel00p today | Initiative |
|------------|--------|----------------|------------|
| Plugin + hooks system | Pre/post tool & LLM hooks; tool/provider/memory plugins discovered from dirs + entry points; last-writer-wins provider override | Providers/tools compiled into the workspace; no extension seam | [#1](plugins-and-hooks.md) |
| Skills (procedural memory) | Agent-authored `SKILL.md` procedures; install from hub (agentskills.io standard); self-improving; curator archives stale skills | Declarative reviewed memory only; no procedures | [#2](skills-system.md) |
| Self-improvement loop | Autonomous skill creation; curator usage tracking + auto-archive with rollback; user modeling (Honcho); cross-session FTS5 + LLM recall | Memory candidate -> review -> approve; no learning loop, no procedure generation | [#3](self-improvement-loop.md) |
| Sub-agents / delegation | `delegate_task` spawns isolated subagents; leaf vs orchestrator roles; concurrent batch | None (single agent loop) — but on roadmap Stage 7 | [#4](subagents-delegation.md) |
| Multi-agent work queue | SQLite kanban; atomic claim/complete; dispatcher; failure auto-block | None | [#4](subagents-delegation.md) |
| Scheduling / cron | `cronjob` tool + `hermes cron`; durations, NL phrases, 5-field cron; per-job model override; runaway interrupt | None | [#5](scheduling-cron.md) |
| Messaging gateway | One core -> Telegram, Slack, Discord, WhatsApp, Teams, Email, SMS, 20+ | CLI + desktop + cloud UI; no chat-platform delivery | [#6](messaging-gateway.md) |
| Execution backends / sandboxing | Local, Docker, SSH, Daytona, Singularity, Modal | Local workspace boundary only; no container/remote isolation | [#7](execution-backends-sandboxing.md) |
| Programmatic tool calling | `execute_code` collapses multi-step tool pipelines into one inference | One tool call per loop iteration | [#8](programmatic-tool-calling.md) |
| Terminal UI | React + Ink TUI over JSON-RPC | None (CLI prints only) | [#9](tui.md) |
| Built-in tool breadth | 60+ tools (web search/browse, image gen, vision, TTS) | read/edit/command/git/MCP; web tools already in backlog | Folded into [#1](plugins-and-hooks.md) + existing backlog |
| Personality / context files | `SOUL.md` + per-project context files | Project instructions from `CODEL00P.md`/`AGENTS.md`/`CLAUDE.md` (partial) | Folded into [#2](skills-system.md) |

## What codel00p already does better (preserve)

- Enterprise provider **policy presets** and credential resolution chains.
- **Clerk Orgs/JWT** governance and a cloud control plane.
- **Reviewed/audited team memory** (Hermes memory is personal and auto-curated).
- **Rust** type-safety and **stable protocol contracts** across surfaces.
- First-class **MCP server** mode (`codel00p mcp serve`).

## Already covered by the existing roadmap

These Hermes capabilities are **not** new initiatives — they are already planned
and should be tracked in [`../roadmap.md`](../roadmap.md):

- Sub-agents and **worktree isolation** — Milestone 7 (this directory adds the
  concrete delegation design in [#4](subagents-delegation.md)).
- **Web fetch/search tools** — Stage 1 "Next work" / agentic backlog #3.
- **Sandboxing / security review** — Milestone 8 (expanded in [#7](execution-backends-sandboxing.md)).
- **Cancellation, interruption, background command monitoring** — Stage 1.

## Out of scope

Hermes's research/RL surface — batch processing, trajectory export, Atropos RL
training integration — does not fit codel00p's product thesis and is not
planned.
