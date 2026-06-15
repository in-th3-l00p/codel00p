# Initiatives: Hermes-Parity Capabilities

This directory holds the planning record for a set of capabilities that Hermes
Agent (NousResearch) ships. The [Product Roadmap](../product-roadmap.md) already
names "Hermes-grade provider breadth" as a goal and already covers some of this
ground (Milestone 7, Multi-Agent Work). These initiatives formalize the
Hermes-distinctive gaps as concrete, phased epics that slot into that roadmap.

**Status (2026-06-15):** Phase 1 of six of these initiatives has now shipped —
plugins & hooks, skills, self-improvement loop, sub-agent delegation,
scheduling/cron, and the messaging gateway each landed core crates on 2026-06-12
(`codel00p-plugin`, `codel00p-skill`, `codel00p-cron`, `codel00p-gateway`, and
the harness `delegation`/`learning` modules), and the TUI shipped Phase 2 plus a
Phase 3 Users tab. Execution backends (#7) and programmatic tool calling (#8)
remain unstarted. The Status column below and each epic's Scope checklist track
the detail.

Start with the [Hermes Gap Analysis](hermes-gap-analysis.md) for the full
comparison and rationale. Each initiative below has its own plan with goal,
current codel00p state, a design mapped to real crates, phased scope, risks, and
exit criteria.

## Initiatives

| # | Initiative | Tier | Maps to roadmap | Status |
|---|------------|------|-----------------|--------|
| 1 | [Plugins & Hooks](plugins-and-hooks.md) | Foundation | New (enables Stage 2/4 extensibility) | Phase 1–2 shipped (`codel00p-plugin`) |
| 2 | [Skills System](skills-system.md) | Foundation | Extends Stage 3 (Memory) | Phase 1 shipped (`codel00p-skill`) |
| 3 | [Self-Improvement Loop](self-improvement-loop.md) | Differentiator | Extends Stage 3 (Memory) | Phase 1 shipped (harness `learning`) |
| 4 | [Sub-Agents & Delegation](subagents-delegation.md) | Primitive | Stage 7 (Multi-Agent) | Phase 1 shipped (harness `delegation`) |
| 5 | [Scheduling / Cron](scheduling-cron.md) | Primitive | New | Phase 1 shipped (`codel00p-cron`) |
| 6 | [Messaging Gateway](messaging-gateway.md) | Reach | New (alongside Stage 6 interfaces) | Phase 1 shipped (`codel00p-gateway`) |
| 7 | [Execution Backends & Sandboxing](execution-backends-sandboxing.md) | Reach | Stage 8 (security/sandboxing) | Planned |
| 8 | [Programmatic Tool Calling](programmatic-tool-calling.md) | Parity | Stage 1 (Agent parity) | Planned |
| 9 | [Terminal UI (TUI)](tui.md) | Interface | Stage 6 (Interfaces) | Phase 2 shipped, Phase 3 started (`codel00p-cli/src/tui`) |

## Sequencing

The initiatives are not independent. The original recommended order (below) is
largely how Phase 1 actually landed — plugins, then skills + self-improvement,
then sub-agents, then scheduling + gateway. It is kept as the rationale and the
guide for the remaining phases:

1. **Plugins & Hooks (#1)** first — it is the substrate. Today every tool and
   provider is compiled into the Rust workspace. Without an extension boundary,
   skills, scheduling, gateway adapters, and execution backends each become a
   core-crate change. Build the seam once.
2. **Skills (#2)** + **Self-Improvement (#3)** next — these are the moat. They
   extend the existing `codel00p-memory` story from *declarative* reviewed
   knowledge into *procedural* reusable capability, which is the core of what
   makes Hermes "grow with you."
3. **Sub-Agents (#4)** — already on the roadmap (Stage 7); a delegation tool
   plus worktree isolation unlocks parallel work and pairs with the kanban work
   queue.
4. **Scheduling (#5)** + **Gateway (#6)** — together these turn codel00p from an
   *invoked tool* into an *always-on agent*. Lower priority for the team/IDE
   positioning, higher priority if codel00p wants Hermes's "lives where you do"
   reach.
5. **Execution backends (#7)**, **Programmatic tool calling (#8)**,
   **TUI (#9)** — quality/breadth layers that can land opportunistically.

## What we deliberately do not copy

codel00p's positioning is **team and enterprise governance** (Clerk Orgs/JWT,
provider policy presets, reviewed/audited memory, stable protocol contracts),
where Hermes is **single-user personal**. Every initiative here is adapted to
preserve that positioning:

- Skills and plugins must respect the permission scopes and audit trail that
  already exist in `codel00p-harness`.
- Self-created skills and memory must stay inside the existing
  candidate -> review -> approve lifecycle, not auto-apply.
- Sub-agents, scheduling, and gateway sessions must emit the same protocol
  events and be visible to the cloud control plane.

We are not adopting Hermes's research/RL surface (Atropos trajectory export, RL
training) — out of scope for the product thesis.

## How to use these docs

- Each initiative is an **epic**, not a single-slice plan. When work starts,
  break the next phase into dated slice plans under
  [`../superpowers/plans/`](../superpowers/plans/) following the existing
  convention, and check items off the epic's Scope list.
- Keep [`../roadmap.md`](../roadmap.md) and
  [`../agentic-backlog.md`](../agentic-backlog.md) as the source of truth for
  active ordering; these initiatives feed into them, they do not replace them.
