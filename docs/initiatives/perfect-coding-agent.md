# Initiative 12: The Perfect Coding Agent

Turn codel00p from "a capable coding *assistant*" into an autonomous coding
*engineer* — one that plans, acts, **verifies its own work before claiming done**,
understands code semantically, and is fully **customizable** (every smart behavior
toggleable and bundleable into profiles).

This plan is grounded in three internal audits (agent loop, code intelligence,
self-awareness surface) and a survey of SOTA harnesses (Claude Code, Aider,
OpenHands/SWE-agent, Cursor/Composer, Cline, Continue, Codex CLI, Devin). It
leans only on *verified* mechanisms, not vendor benchmark marketing.

## Where codel00p already leads

It is already ahead of most harnesses on the things they add late:
per-call permission governance + audit, reviewed memory, reviewed **capability
synthesis** (frozen tools — unique), governed `execute_code`, execution backends
(local / Docker±warm / SSH) with worktree-isolated sub-agents, and a robust
4-strategy fuzzy patch matcher with atomic multi-file edits. **Agent
self-awareness** (identity/capabilities/run-state injection + `self_describe` +
`[agent.behavior]` toggles) shipped in #94.

## The gap to "perfect"

Concentrated in two areas the audits made sharp:

1. **Autonomy discipline.** The turn loop ends when the model stops calling tools
   or hits `max_iterations` (default was 4) — *nothing verifies the claim*. There
   is no required planning and (without a `CODEL00P.md`) no base rigor prompt.
   This is the literal cause of the dogfooding failure where a web app's unit
   tests were green while the app was browser-broken.
2. **Semantic code intelligence.** Navigation is purely lexical (regex repo-map +
   grep; no tree-sitter / LSP / embeddings), and no build/test/workspace state is
   injected into context.

## Design principle: customizable, smart-by-default

Every capability below is an individual `[agent.behavior]` toggle with a smart
default (usually on), bundleable into user-defined `[agent.profiles.<name>]`
profiles, with shipped presets (`autonomous` / `careful` / `manual`) and
org-pinning (like the require-isolation policy). "Make it smarter" and "let me
turn the smarts off" are answered together. `[agent.behavior]` was bootstrapped
by #94 (`self_knowledge`, `self_state`).

---

## Tier 0 — Autonomy discipline (highest leverage; in progress)

- [ ] **Verify-before-done loop** — after mutating turns, run the project's
      test/build/lint and feed failures back into the loop instead of completing;
      support acceptance criteria ("done = green"). (Aider lint-and-fix;
      OpenHands/SWE-agent reproduce→fix→verify.)
      → `agent.behavior.auto_test`, `self_verify`, `lint_and_fix` (default on);
      `test_command`, `verify_iterations`.
- [ ] **Metacognition / self-critique** (self-awareness facet 3) — before
      declaring done: "what did I claim, did I verify it, what's untested/risky?"
      Shares machinery with the verify loop. → `agent.behavior.self_critique`.
- [ ] **Planning rigor + base system prompt** — ship a base prompt encoding
      plan → act → verify → reflect; make planning expected for multi-step work;
      raise the `max_iterations` default well above 4. → `agent.behavior.auto_plan`.
- [ ] **Error self-correction loop** — classify failures (missing dep / perms /
      timeout — Codex matches denial signatures), add a retry/failure budget,
      surface full failing output. → `agent.behavior.replan_on_failure`,
      `failure_budget`.

## Tier 1 — Semantic code intelligence (lexical → semantic)

- [ ] **Tree-sitter repo map** — real ASTs instead of regex extraction; keep the
      PageRank-style ranking, run it on real symbols (Aider's design).
- [ ] **LSP integration** — go-to-def, find-references, hover, diagnostics; inject
      diagnostics / compile errors into context.
- [ ] **Workspace/build/test-awareness context block** — detect build/test
      commands (Cargo.toml/package.json/…), inject git status, recently-edited
      files, last test result, live diagnostics.
- [ ] **Hybrid retrieval** for memory and code — BM25 + embeddings +
      recently-edited + repo-map, reranked (Continue's verified 4-retriever
      design). Upgrades memory from lexical-only; adds semantic code retrieval.

## Tier 2 — Trust & scale

- [ ] **Checkpoints / rollback** — per-step shadow-git snapshots (Cline's
      `core.worktree`-redirect trick, `--allow-empty --no-verify`, 3 restore
      modes) so aggressive autonomy is safely undoable.
- [ ] **Browser / Playwright verification hook** — a CDP/Playwright tool so the
      agent can verify web work like a human (directly addresses the dogfooding
      gap where green API tests hid a browser-broken app).
- [ ] **Auto-decomposition + parallel sub-agent fan-out** — a planner/coordinator
      that splits work and fans out over the existing worktree-isolated sub-agents
      (Cursor 2.0 ≤8 worktrees; Devin "Managed Devins" isolated VMs).
- [ ] **Faster/robust apply** — per-model SEARCH/REPLACE markers (Cline reported
      +10% edit success); optional speculative fast-apply.

## Tier 3 — Harden existing strengths

- [ ] **LLM-distilled context compaction** (today: naive line-listing).
- [ ] **Sandbox matrix + protected-metadata carveouts** — keep `.git`/`.codel00p`
      read-only even in writable roots; on-request escalation on denial signatures
      (Codex internals).
- [ ] **Smarter memory** — semantic dedup + proactive task-aware recall.

## Cross-cutting

- [ ] **Profiles** — `[agent.profiles.<name>]` table-of-tables + shipped presets +
      `--profile` + org-pinning, layering over `[agent.behavior]`.
- [x] **Self-awareness** facets 1+2 (self-knowledge + run-state) shipped (#94);
      facet 3 (metacognition) lands with Tier 0's verify loop.

## Verified-vs-marketing note

Verified mechanisms this plan relies on: shadow-git checkpoints, Codex sandbox
internals, Continue's 4-retriever pipeline, tree-sitter repo maps, Cline's
marker-aware diff apply. Treated as marketing (no public methodology) and NOT
relied upon: Windsurf recall multipliers, Cursor/Devin headline benchmark
percentages.
