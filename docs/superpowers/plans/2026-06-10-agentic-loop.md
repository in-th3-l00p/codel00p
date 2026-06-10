# Agentic Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a durable repo-native operating loop for continuous codel00p development.

**Architecture:** The loop is documentation-backed, with one policy file that defines cycle mechanics and one backlog file that orders product slices. Future implementation plans should cite these files and produce one verified commit per cycle.

**Tech Stack:** Markdown docs, existing roadmap docs, git, `pnpm verify`.

---

### Task 1: Document The Operating Loop

**Files:**
- Create: `docs/agentic-loop.md`
- Create: `docs/agentic-backlog.md`
- Create: `docs/superpowers/plans/2026-06-10-agentic-loop.md`

- [x] **Step 1: Write loop contract**

Define the cycle order, hard gates, priority order, required output, and stop
conditions in `docs/agentic-loop.md`.

- [x] **Step 2: Write active backlog**

Define the current work queue in `docs/agentic-backlog.md`, ordered as:
provider route intelligence, Memory 2.0, agent parity hardening, MCP
certification, team cloud and sync, desktop/cloud interfaces.

- [x] **Step 3: Run documentation sanity checks**

Run:

```bash
rg -n "coauthor|pnpm verify|Provider Route Intelligence|Memory 2.0" docs/agentic-loop.md docs/agentic-backlog.md
```

Expected: the loop docs mention verification, clean commit rules, and the top
active backlog priorities.

- [ ] **Step 4: Commit and push**

Run:

```bash
git add docs/agentic-loop.md docs/agentic-backlog.md docs/superpowers/plans/2026-06-10-agentic-loop.md
git commit -m "docs: add agentic development loop"
git push origin main
```
