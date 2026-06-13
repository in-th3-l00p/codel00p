# codel00p — Pitch

**Coding agents that get smarter the more your team uses them.**

## The problem

Every agent session starts cold. It relearns your repo layout, your
conventions, your deploy steps, and the same gotchas — over and over. The
knowledge a senior engineer carries in their head never reaches the tools, so
the agent stays junior forever and that context dies in chat logs.

## The insight

The durable advantage in agentic coding isn't the model — everyone rents the
same frontier models. It's the **harness, the context, and the memory** around
it. codel00p makes project knowledge a first-class, **reviewed, shareable**
asset: completed work becomes compact memory — codebase facts, architecture
decisions, workflows, debugging procedures, deploy/rollback steps — that is
curated like code (proposed, reviewed, approved), fed into every future session,
and shared across the whole team.

## Why it wins

- **Knowledge-first** — the memory layer is the moat, not a bolt-on. Agents
  start *warm*.
- **Provider-flexible** — one Rust provider contract spanning Anthropic, OpenAI,
  Azure AI Foundry, AWS Bedrock, Gemini, OpenRouter, and custom gateways, with
  policy, credentials, and fallback decoupled from memory. Personal keys or
  org-governed ones.
- **Team control plane** — Clerk-org-scoped shared pools of projects, agents,
  and MCP configs, with reviewed memory synced across the organization.
- **Open-source by default** — fully inspectable and hackable; cloud makes teams
  stronger without locking them in.

## What's real today

Not slideware — this is running:

- A **Rust agent engine + CLI** that inspects, edits, tests, commits, resumes,
  streams tokens live, and connects to MCP servers. Bare `codel00p` opens an
  interactive chat.
- A **cloud control plane** (axum + Postgres) **deployed on Fly.io**, with full
  org-scoped CRUD for projects/agents/MCP, a memory review queue, and live
  server-sent-event updates — persistence verified end to end in production.
- An **Electron desktop app** and a **Next.js web dashboard**, with Clerk
  sign-in across web, desktop, *and* CLI (`codel00p login` browser flow).
- Memory push/pull sync, stored agent definitions you can run, and prebuilt
  release binaries with a curl installer.

## Where it's going

Memory 2.0 (semantic retrieval, dedup, staleness detection, revision history),
organization provider policy with usage/budget/audit, MCP certification against
the tools teams actually use, and coordinated multi-agent long-horizon work. See
the [Roadmap](roadmap.md) and [Product Roadmap](product-roadmap.md).

---

> **In one line:** codel00p turns your team's real work into reviewed, durable,
> shareable memory — so your coding agent compounds in value instead of starting
> from zero every time.
