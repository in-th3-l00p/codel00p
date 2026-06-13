# codel00p

codel00p is an open-source agentic coding platform built around a simple
premise: **a team's project knowledge should grow as the team works**.

The final codel00p product will combine an agent harness, CLI, desktop app,
cloud platform, provider routing, and shared project memory into a polished
developer experience. The project is intentionally split into smaller
subprojects so each layer can be designed, tested, and released independently.

## Why this exists

Coding agents are useful, but most sessions start with weak project context.
They relearn the same repository structure, conventions, deployment details,
debugging patterns, and decisions again and again.

codel00p is focused on making that knowledge durable. It helps teams turn real
work into compact, reviewed, reusable memory that improves future agent
sessions and gives teammates a shared understanding of the codebase.

Cloud matters because team knowledge is more powerful when it is shared. The
cloud platform should make project memory available across an organization,
centralize provider access, and give teams governance over how agents are used
in real engineering workflows.

## Subprojects

The codel00p ecosystem is planned as a set of open-source projects:

- **codel00p-memory:** project memory engine.
- **codel00p-harness:** agent runtime.
- **codel00p-cli:** first developer-facing interface.
- **codel00p-desktop:** Electron control center.
- **codel00p-cloud:** organization and team platform.
- **codel00p-sync:** memory synchronization.
- **codel00p-providers:** inference provider abstraction.
- **codel00p-protocol:** shared contracts between modules.
- **codel00p-storage:** backend-neutral local and cloud storage contracts.
- **codel00p-research:** harness and memory experiments.
- **codel00p:** the final integrated product.

See [Subprojects](docs/subprojects.md) for the full technical split.

## Operating model

codel00p should support cloud and local operation:

- cloud workspaces provide shared memory, organization governance, provider
  access, and team visibility;
- local runtimes support private or offline work when teams need it;
- users and organizations can choose the provider strategy that fits their
  project, cost, privacy, and compliance needs.

The product should not force an ideology around where agents run. The durable
asset is the project knowledge that codel00p captures and makes reusable.

## Documentation

- [Pitch](docs/pitch.md)
- [Project Description](docs/project.md)
- [Configuration](docs/configuration.md)
- [Product Roadmap](docs/product-roadmap.md)
- [Subprojects](docs/subprojects.md)
- [Repository Structure](docs/repository.md)
- [Dependency Policy](docs/dependencies.md)
- [Architecture](docs/architecture.md)
- [Protocol](docs/protocol.md)
- [Storage](docs/storage.md)
- [Inference Providers](docs/providers.md)
- [Agent Harness](docs/harness.md)
- [Agent Capability Parity](docs/agent-capability-parity.md)
- [MCP Compatibility](docs/mcp-compatibility.md)
- [Project Memory](docs/memory.md)
- [Memory Development Plan](docs/memory-development.md)
- [Roadmap](docs/roadmap.md)
- [Contributing](CONTRIBUTING.md)

## Install

Prebuilt binaries are published on each tagged release. Install the CLI with one
command:

```bash
# macOS and Linux
curl -fsSL https://raw.githubusercontent.com/in-th3-l00p/codel00p/main/install.sh | sh
```

```powershell
# Windows (PowerShell)
irm https://raw.githubusercontent.com/in-th3-l00p/codel00p/main/install.ps1 | iex
```

Prefer to build it yourself? With a recent Rust toolchain:

```bash
git clone https://github.com/in-th3-l00p/codel00p
cd codel00p/core
cargo build --release --bin codel00p
```

## Repository layout

```text
core/      Rust workspace for the codel00p engine crates
apps/      deployable user-facing applications
packages/  shared TypeScript packages
docs/      product, architecture, and research documentation
tools/     repository automation and fixtures
```

Rust development happens from `core/`:

```bash
cd core
cargo test --workspace
```

## Configuring the CLI

codel00p reads layered TOML configuration from `~/.codel00p/config.toml` (and an
optional per-project `./.codel00p/config.toml`), so commands run without
repeating flags. Run `codel00p config setup` once to choose a provider, store a
key, and pick a model; then `codel00p agent chat` needs no arguments. Manage it
with `codel00p config` and `codel00p providers` — see
[Configuration](docs/configuration.md).

Application development uses the root pnpm workspace:

```bash
pnpm install
pnpm typecheck
pnpm build
pnpm dev:landing
pnpm dev:desktop
```

## Deployment

The control surfaces run as live services:

- **Web** — `apps/landing` is one Next.js app serving the marketing site, the
  documentation, and the authenticated cloud control surface (custom Clerk
  sign-in plus an organization dashboard for projects, agents, and MCP servers).
  It deploys to **Vercel** (production from `main`).
- **Backend** — `core/crates/codel00p-cloud` is the Rust (axum) control-plane
  API, deployed to **Fly.io** with **Postgres**. It verifies Clerk session JWTs
  and is the source of truth for organization data; the web app reaches it via
  `@codel00p/sdk` over `CODEL00P_API_URL`.
- **CI/CD** — GitHub Actions verifies the Rust core and the apps on every push,
  and auto-deploys `codel00p-cloud` to Fly when `core/` changes on `main`; Vercel
  handles web deploys.

## Current implementation

The first Rust modules have started:

- [codel00p-memory](core/crates/codel00p-memory): candidate creation, review
  lifecycle, audit history, and deterministic retrieval for approved project
  knowledge.
- [codel00p-cli](core/crates/codel00p-cli): layered TOML configuration
  (`codel00p config` / `codel00p providers`) so commands run without flags,
  terminal agent runs, interactive multi-turn chat sessions with resumable
  history, in-session tools, and live token streaming, conversation listing,
  durable session inspection, memory review commands for listing, inspecting,
  approving, rejecting, archiving, and auditing memory records, and
  `codel00p cloud` commands to push local memory to a team review queue, pull
  approved team memory back into the local store, and run a stored cloud agent
  (resolving its config, MCP servers, and RAG context, then executing a turn).
- [codel00p-providers](core/crates/codel00p-providers): provider registry,
  high-level inference client, OpenAI-compatible Chat Completions transport,
  Azure Chat Completions transport, Anthropic Messages transport, OpenAI
  Responses transport, AWS Bedrock Converse transport, Gemini GenerateContent
  transport, native token streaming for every transport (SSE plus the Bedrock
  binary event stream), tool calls, route inspection, and provider policy
  enforcement.
- [codel00p-harness](core/crates/codel00p-harness): agent turn loop,
  workspace-safe read/edit/command/git tools, project instructions,
  permissions, compaction primitives, deterministic events, model-client
  boundary, and provider adapter.
- [codel00p-protocol](core/crates/codel00p-protocol): shared data contracts for
  sessions, turns, events, tool calls, providers, projects, and memory entries.
- [codel00p-storage](core/crates/codel00p-storage): backend-neutral storage
  primitives for scoped key-value state, documents, and append logs, with
  in-memory, SQLite, and Postgres backends behind one `StorageBackend` trait.
- [codel00p-cloud](core/crates/codel00p-cloud): the team control-plane HTTP
  service (axum) — Clerk-authenticated organizations as shared pools of projects,
  agents, MCP servers, and a RAG knowledge base. Full CRUD for projects/agents/
  MCP servers, the shared memory review queue (push, list, approve/reject, audit),
  and RAG retrieval over approved memory; org-admin write access; persisting
  through `codel00p-storage`.
- [codel00p-session](core/crates/codel00p-session): durable session metadata and
  replay built on top of `codel00p-storage`.
- [codel00p-mcp](core/crates/codel00p-mcp): MCP client/server runtime,
  stdio/HTTP transports, tools, resources, prompts, subscriptions, diagnostics,
  and harness event routing for external team tools.

## Current status

codel00p now has a working CLI vertical slice: an agent can call
Chat-Completions-compatible providers plus native OpenAI Responses, Anthropic
Messages, AWS Bedrock Converse, and Gemini GenerateContent transports; inspect
and modify a workspace through permissioned tools; run commands; inspect git
state; persist and resume sessions; stream events; connect to MCP tools; extract
candidate memories; and reuse approved project memory in future runs.

The next production work is hardening the product around richer agent parity,
provider breadth, memory quality, third-party MCP certification, and then the
desktop/cloud control surfaces for team usage, governance, and shared memory.
