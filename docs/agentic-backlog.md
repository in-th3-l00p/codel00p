# Agentic Backlog

This backlog is the working queue for the agentic loop. It is ordered by current
leverage, not by final product importance.

The larger Hermes-parity capabilities are tracked as phased epics under
[Initiatives](initiatives/README.md). **Phase 1 of most has now shipped**
(plugins & hooks, skills, self-improvement loop, sub-agent delegation,
scheduling/cron, messaging gateway — the 2026-06-12 wave — plus the TUI), and the
2026-06-16 wave **completed Tool-Calling Parity (#10)** across all three phases
(which also delivered programmatic tool calling, #8) and opened **Capability
Synthesis (#11)**; only **execution backends & sandboxing (#7)** is unstarted. As
each initiative's next phase begins, break it into dated slice plans under
`superpowers/plans/` and surface the active slice here.

## Session marker (2026-06-17)

Latest release: **v0.3.0**. The 2026-06-16 wave effectively **completed
Tool-Calling Parity (#10)**: Phase 1 (schema serialization, arg validation,
result truncation, `tool_choice`), Phase 2 (progressive disclosure
`tool_search`/`tool_describe`, default sub-agent spawner, `find_files`/`grep`,
`repo_map`, background command execution, the `update_plan` working-plan tool),
and Phase 3 (`run_pipeline` programmatic tool calling, JSON mode /
`ResponseFormat`) all landed on `main` — along with a new initiative,
**Capability Synthesis (#11)** slices 1–2 (freeze a governed pipeline into a
reviewed tool; auto-extraction + verification gate), and the tolerant atomic
multi-edit `apply_patch` engine.

Only two small tool-calling tails remain: **token-efficient tool results**
(Phase 2) and **streaming tool-call arguments** (Phase 3).

**2026-06-17 wave** advanced #3 and #4: for **Memory 2.0 (#3)**, shipped the
**`memory_merge` MCP tool + `memory split` workflow** (repository/CLI/MCP, PR #37)
and **semantic (lexical) memory retrieval ranked behind deterministic filters**
(repository/CLI/MCP, reusing the existing Jaccard similarity scorer, PR #39); for
**Agent Parity Hardening (#4)**, shipped the **`prepare_pr` workflow tool**
(offline PR-ready summary from git state, PR #36) and **deterministic context
manifests** (the `ContextManifest` event with a content hash, PR #38).

**Next priority:** **Agent Parity Hardening (#4)** now needs only
**worktree-isolated execution** (in progress). The rest of **Memory 2.0 (#3)** is
post-session recommendations, imports, richer revision history, source evidence,
and visibility scopes. **Provider Route Intelligence (#2)** remains blocked on
cloud product/design context. The longer-horizon frontiers are **Execution
Backends & Sandboxing (#7)** — the blocker for true `execute_code` — and
Capability Synthesis's next slices (org propagation, composition).

Active follow-up: **TUI writable org switching** — Clerk token re-mint primitive
shipped; the current slice wires the Org tab to a real organization picker and
`auth login --org` (see [TUI initiative](initiatives/tui.md) Phase 3).

## Active Priority

### 1. Tool-Calling Parity — SHIPPED (two tails remain)

Goal: bring tool calling up to Claude Code / OpenCode / Hermes / OpenHands level
without losing codel00p's permission scopes and audit events. See the
[Tool-Calling Parity initiative](initiatives/tool-calling-parity.md) for the full
gap analysis and phased plan (research pass 2026-06-15).

The audit that opened this priority found the model never received tool parameter
schemas — `provider_adapter.rs` sent every tool as `{"type":"object"}`, discarding
the hand-written `Tool::input_schema()`/`description()`. That floor, plus the
surface/orchestration and SOTA layers on top of it, are now built.

**Phase 1 (floor) — SHIPPED 2026-06-15**, all green on `main`:

- ~~serialize real tool schemas + descriptions to providers~~ (PR #13);
- ~~validate tool arguments against the schema before dispatch~~ (PR #14);
- ~~tool-result truncation with persist-to-disk + preview~~ (PR #15);
- ~~`tool_choice` control (auto / required / specific / none) across every
  transport~~ (PR #16).

**Phase 2 (surface/orchestration) — SHIPPED 2026-06-16:**

- ~~MCP tool search / progressive disclosure (`tool_search` / `tool_describe`)~~;
- ~~default sub-agent spawner so `delegate_task` works out of the box~~;
- ~~`find_files` (glob) and `grep` (regex) navigation~~;
- ~~`repo_map` ranked code-symbol map~~;
- ~~background command execution + the `update_plan` working-plan tool~~.

**Phase 3 (SOTA) — SHIPPED 2026-06-16:**

- ~~programmatic tool calling (`run_pipeline`)~~;
- ~~JSON mode / structured output (`ResponseFormat`)~~;
- ~~capability synthesis slices 1–2~~ — now its own initiative,
  [#11](initiatives/capability-synthesis.md).

**Remaining tails:** token-efficient tool results (pagination/filter +
concise/detailed `response_format` on the heavy tools) and streaming tool-call
arguments. Arbitrary in-process `execute_code` still waits on
[Execution Backends (#7)](initiatives/execution-backends-sandboxing.md).

### 2. Provider Route Intelligence

Goal: make provider routing policy-aware, auditable, and ready for team control.

Build:

- model capability metadata; started with raw catalog labels plus typed tools,
  streaming, vision, and reasoning flags;
- route audit metadata; started with safe `ResolvedInferenceRoute` fields for
  auth type, base URL source, policy decision, model catalog URL, and provider
  capabilities, plus credential source/kind/source-kind labels and route policy
  snapshots, and a provider `Retry-After` hint captured from rate-limit/overload
  responses and surfaced as `retry_after_secs` in failed route-attempt metadata;
- model catalog listing with normalized model descriptors and safe credential
  source/kind/source-kind, auth type, and catalog URL source metadata;
- fallback routing for retryable failures with ordered route-attempt metadata,
  plus bounded retry-with-backoff on the same route (exponential delay + jitter,
  honoring a provider `Retry-After` hint, capped) before falling back;
- normalized usage and explicit request-priced cost metadata, including safe
  pricing source labels;
- provider and model allowlist policy hooks for project and organization rules;
  started with catalog filtering by required typed model capabilities and safe
  catalog policy metadata, provider-scoped auth type, credential
  kind/source-kind, and capability rules, plus enterprise cloud-proxy,
  custom-gateway, managed-identity, organization-credential, and direct agentic
  templates, sparse policy JSON serialization for control-plane defaults, and
  named built-in preset metadata, CLI preset selection, and shared TypeScript
  protocol/SDK preset exports for UI/config selection.

Why now: native provider execution is implemented, so routing quality is the
next bottleneck before cloud-managed providers and team usage controls.

Started enterprise credential groundwork with typed credential source kind
metadata, managed-identity credential injection, a resolver boundary for
managed identity token providers, and offline-tested Azure IMDS, AWS IMDSv2,
and GCP metadata server token acquisition, plus default-off live smoke test
scaffolding for cloud runtimes. Added enterprise organization-credential and
custom-gateway policy templates for team-managed provider credentials and
configured gateway rollouts, then made `ProviderPolicy` safely serializable for
cloud and desktop defaults, exposed stable built-in preset metadata, wired CLI
agent runs to select presets by ID, and mirrored the preset catalog through
TypeScript protocol/SDK packages for future cloud and desktop configuration
surfaces.

### 3. Memory 2.0

Goal: make project memory trusted, maintainable, and visibly sourced.

Build:

- memory editing and revision history; started with CLI/MCP-backed content
  edits that preserve metadata and append CLI/MCP-visible `edited` audit
  events with memory id and previous/new content metadata, plus CLI/MCP restore
  from edit audit revisions;
- deterministic approved-memory retrieval; CLI/MCP substring search over
  approved project memory by text, kind, tag, and limit, **plus `memory retrieve`
  / `memory_retrieve` (PR #39): query-string → approved-memory results ranked by
  the existing Jaccard similarity scorer behind deterministic filters (kind, tag,
  sensitivity, limit), with deterministic id tie-breaking** — offline, no
  embeddings;
- source evidence links; started with CLI detail text plus CLI
  list/show/search/similar JSON source session/turn metadata, replay
  `source_uri`, and MCP
  show/resource/list/search/similar JSON source metadata;
- duplicate and near-duplicate detection; started with exact active duplicate
  candidate rejection, repository-level active memory similarity scoring, and
  CLI/MCP review access to near-duplicate scores;
- merge/split workflows; repository `merge(source, target)` archives the
  duplicate source, carries its tags onto the surviving target, and writes a
  two-sided `merged` audit trail — surfaced over **CLI and MCP** (`memory_merge`,
  PR #37). **`memory split` (PR #37)** is the inverse: it creates a new candidate
  from part of a source's content (carrying project/kind/sensitivity/tags + a
  source reference), optionally updates the source content, and writes a two-sided
  `split` audit trail using a dedicated `split_into` field; surfaced over
  repository, CLI (`codel00p memory split <source> <new_id> --content ...`), and
  MCP (`memory_split`);
- stale-memory detection; started with repository-level newer active-memory
  overlap scoring for approved memories and CLI/MCP stale review output;
- visibility and sensitivity scopes; started with normal/sensitive memory
  labels, default normal-only retrieval, and explicit CLI/MCP sensitive-memory
  review/search filters;
- memory quality scoring; started with deterministic `MemoryRecord::quality()`
  advisory scores and findings for short, long, or vague memory content, now
  exposed in CLI/MCP memory JSON records for review surfaces, plus a core
  low-quality active-memory review query surfaced through CLI and MCP with
  status-, kind-, sensitivity-, and tag-scoped review filters.

Why next: memory is the product moat. Provider breadth matters, but durable
reviewed knowledge is what makes codel00p distinct for teams.

### 4. Agent Parity Hardening

Goal: close the remaining gap with mature coding agents.

Build:

- ~~richer patch/diff engine~~ (tolerant atomic multi-edit `apply_patch`);
- ~~cancellation and interruption~~ (cooperative `CancelSignal`);
- ~~background command monitoring~~ (`run_command background:true` + `process_*`);
- ~~web fetch/search tools~~;
- worktree-isolated execution **(in progress)** — mutating sub-agents in isolated
  git worktrees;
- ~~PR preparation workflow~~ (`prepare_pr` tool, PR #36);
- ~~most-recent-session continue UX~~ (`agent continue`);
- ~~deterministic context manifests~~ (`ContextManifest` event + content hash,
  PR #38).

Only **worktree-isolated execution** remains.

### 5. MCP Certification

Goal: make external integrations predictable for real teams.

Build:

- compatibility matrix against common third-party MCP servers;
- regression fixtures for discovered edge cases;
- OAuth and authenticated connector flows;
- large tool-set search and filtering;
- connector policy templates.

### 6. Team Cloud And Sync

Goal: centralize project memory, provider policy, usage, permissions, and audit.

Build:

- organization, team, role, project, and invitation models;
- organization-managed provider credentials;
- shared memory sync;
- usage, cost, and budget views;
- cloud storage backend behind `codel00p-storage`;
- sync conflict representation and resolution.

### 7. Desktop And Cloud Interfaces

Goal: expose the agent, memory, and governance loop visually.

Shipped: a full-screen **terminal UI** (`ratatui`) — streaming transcript,
tool timeline, inline permission-approval modal, model picker, and a read-only
org entity browser. See [TUI initiative](initiatives/tui.md).

Next TUI slices (each independently shippable/testable):

- ~~backend `GET /org/members` route (Clerk Backend API) + `OrgMember` protocol
  type + `CloudClient::list_org_members` → real Users tab~~ **shipped
  2026-06-15** (`feat/tui-org-members-users-tab`);
- Clerk token re-mint in `login.rs` → **writable org switching** from the Org tab
  (token re-mint primitive shipped; current slice wires the Org tab picker +
  `auth login --org`);
- model picker sourced from a provider `list_models` call (today: static catalog
  in `tui/app.rs`);
- TUI mouse support, configurable themes, token/usage meters; session switcher.

Still to build (desktop/web):

- desktop session supervision timeline;
- desktop/web permission approval UI (terminal one shipped);
- memory candidate review queue;
- project knowledge browser;
- provider status and configuration views;
- team activity views.

### 8. Release & Self-Update (shipped — follow-ups)

Goal: let installed CLIs detect and install new releases instantly.

Shipped: `codel00p update` (SHA-256-verified in-place replace), daily background
check + nudge (CLI line + TUI header chip), `codel00p version`, tag-stamped
release binaries (`build.rs` + release.yml). Verified end to end v0.1.0 → v0.2.0;
v0.3.0 updates the release lookup to avoid unauthenticated GitHub REST API rate
limits.
**Release = bump `codel00p-cli` Cargo.toml version, commit, push a `vX.Y.Z` tag.**

Follow-ups:

- Windows self-update (today points at `install.ps1`);
- `cargo-dist` or similar to keep installer/updater/asset names in lockstep;
- release channels (stable/beta) once cadence justifies it.

## Selection Rule

Each loop cycle should choose the smallest slice that:

- advances the highest active priority;
- can be tested without live credentials unless explicitly marked as live-only;
- can be committed independently;
- leaves the repo closer to production use.
