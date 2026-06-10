# codel00p-cli

Terminal interface for codel00p.

The current CLI is the first usable codel00p product slice: a coding agent with
durable sessions, reviewed project memory, and explicit tool-set opt-ins. It is
read-only by default.

Commands open one SQLite-backed store scoped by organization and project. That
store currently holds memory records and session replay records.

## Global Options

```bash
codel00p \
  --memory-db .codel00p/memory.sqlite \
  --organization-id org-1 \
  --project-id project-1 \
  --project-name codel00p \
  <command>
```

## Agent Run

`agent run` executes one harness turn against a workspace. It supports
OpenAI-compatible Chat Completions providers plus native OpenAI Responses,
Anthropic Messages, AWS Bedrock Converse, and Gemini GenerateContent transports.
That covers GitHub Copilot, GitHub Models, OpenRouter, Azure AI Foundry
compatible endpoints, custom/local OpenAI-compatible endpoints, OpenAI,
Anthropic, AWS Bedrock, and Google Gemini.

```bash
CODEL00P_PROVIDER_CUSTOM_API_KEY=local-dev-key \
codel00p \
  --memory-db .codel00p/memory.sqlite \
  --organization-id org-1 \
  --project-id project-1 \
  --project-name codel00p \
  agent run "Inspect this repository and summarize the architecture." \
  --workspace . \
  --provider custom \
  --model local-model \
  --base-url http://127.0.0.1:11434/v1 \
  --session-id session-architecture
```

The default tool registry exposes only read-only workspace tools. Add
`--tool-set` to opt into stronger capabilities for the current run:

```bash
codel00p ... agent run "Implement the next tested slice." \
  --provider custom \
  --model local-model \
  --tool-set edit \
  --tool-set command \
  --tool-set git
```

Supported tool-set names:

- `read`, `read-only`, `readonly`: keep the default read-only tools.
- `edit`, `editing`, `write`: file creation, update, deletion, and patching.
- `command`, `commands`, `shell`: structured command execution.
- `git`: guarded status, diff, log, and commit tools.
- `all`: enable edit, command, and git tool sets.

Attach external MCP stdio tools with `--mcp-server <id=command>`. The command
is parsed into an executable, argv, and optional leading environment
assignments; it is spawned directly, not through a shell. The CLI starts the
server, performs the MCP initialization handshake, discovers `tools/list`, and
exposes tools as `mcp.<id>.<tool>`.

```bash
codel00p ... agent run "Search project docs." \
  --provider custom \
  --model local-model \
  --mcp-server "docs=DOCS_ROOT=. ./tools/docs-mcp-server --watch=false"
```

Workspace MCP servers can also be declared in `.codel00p/mcp.json`:

```json
{
  "servers": {
    "docs": {
      "command": "./tools/docs-mcp-server",
      "args": ["--watch=false"],
      "env": { "DOCS_ROOT": "." },
      "timeoutMs": 30000,
      "permissionScope": "external_connector",
      "toolScopes": {
        "search": "read_only"
      }
    }
  }
}
```

HTTP MCP endpoints use `url` instead of `command`:

```json
{
  "servers": {
    "remote-docs": {
      "url": "https://mcp.example.com/mcp",
      "bearerTokenEnv": "REMOTE_DOCS_MCP_TOKEN",
      "headers": { "X-Workspace": "docs" },
      "timeoutMs": 30000
    }
  }
}
```

`permissionScope` applies to every tool from a server. `toolScopes` overrides a
single discovered tool. Supported scopes are `read_only`, `workspace_write`,
`shell`, and `external_connector`. MCP tools default to `external_connector`
when no scope is configured.

Validate configured MCP tools without a model call:

```bash
codel00p ... agent mcp list --workspace .
```

Expose codel00p itself as a stdio MCP server for other agents/tools:

```bash
codel00p ... mcp serve
```

The server exposes `memory_similar`, `memory_search`, `memory_list`,
`memory_show`, `memory_create_candidate`, `memory_approve`, `memory_reject`,
`memory_archive`, `memory_edit`, `memory_restore`, and read-only `session_show`
tools backed by the same project-scoped memory/session database. Memory writes
keep the review lifecycle: external clients create candidates first, then
explicitly approve, reject, or archive them.

It also exposes JSON resources for clients that browse context directly:

- `codel00p://memory/{id}` reads one project memory record.
- `codel00p://sessions/{session_id}` reads one session replay.

Clients can subscribe to memory resource URIs with `resources/subscribe`.
Successful memory create, approve, reject, and archive tool calls emit
`notifications/resources/updated` for matching subscriptions.

Requests to `tools/call` and `resources/read` that include
`_meta.progressToken` receive `notifications/progress` before the final
JSON-RPC response.

Tool calls run with `--permission-mode allow` by default. Use
`--permission-mode deny` to exercise a turn without mutating the workspace or
running commands; denied calls are returned to the model as structured tool
results. Use `--permission-mode ask` to approve or reject each requested tool
call from stdin; prompts are printed on stderr so stdout remains scriptable.
When no approval input is available, `ask` denies the call.

Use `--remember-permissions` with `--permission-mode ask` to persist MCP
connector allow/deny decisions in the project-scoped local store. Later runs
with the same flag reuse remembered decisions for the same MCP tool and
permission scope without prompting again.

Inspect and revoke remembered connector decisions:

```bash
codel00p ... mcp permissions list
codel00p ... mcp permissions forget mcp.docs.search --scope external_connector
```

`mcp permissions list` prints tab-separated `tool`, `scope`, and `status`
columns so shell scripts can audit project connector policy directly.

Use `--stream-events` when a caller wants one JSON event per line as the turn is
running. Use `--json-events` when a caller wants the final assistant text first
and the complete event list after the turn completes.

Provider credentials are read from environment variables:

- GitHub Copilot (`github`): `CODEL00P_PROVIDER_GITHUB_TOKEN`,
  `COPILOT_GITHUB_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN`.
- GitHub Models (`github-models`): `CODEL00P_PROVIDER_GITHUB_MODELS_TOKEN`,
  `GITHUB_TOKEN`, `GH_TOKEN`.
- OpenAI: `CODEL00P_PROVIDER_OPENAI_API_KEY`, `OPENAI_API_KEY`.
- Anthropic: `CODEL00P_PROVIDER_ANTHROPIC_API_KEY`, `ANTHROPIC_API_KEY`,
  `ANTHROPIC_TOKEN`.
- AWS Bedrock: `CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID`,
  `CODEL00P_PROVIDER_AWS_SECRET_ACCESS_KEY`,
  `CODEL00P_PROVIDER_AWS_SESSION_TOKEN`, `CODEL00P_PROVIDER_AWS_REGION`,
  `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`,
  `AWS_REGION`, `AWS_DEFAULT_REGION`.
- OpenRouter: `CODEL00P_PROVIDER_OPENROUTER_API_KEY`, `OPENROUTER_API_KEY`.
- Azure AI Foundry compatible: `CODEL00P_PROVIDER_AZURE_FOUNDRY_API_KEY`,
  `AZURE_FOUNDRY_API_KEY`.
- Gemini: `CODEL00P_PROVIDER_GEMINI_API_KEY`, `GOOGLE_API_KEY`,
  `GEMINI_API_KEY`.
- Custom/local OpenAI-compatible: `CODEL00P_PROVIDER_CUSTOM_API_KEY`.

The CLI uses the shared provider registry resolver for these credentials. Route
metadata records safe source labels such as
`environment:CODEL00P_PROVIDER_OPENAI_API_KEY`, never the secret value.

`openai`, `anthropic`, `bedrock`, and native `gemini` registry entries exist in
the provider layer. `agent run` currently rejects non-Chat-Completions modes
until the CLI agent loop is enabled for each native transport.

## Agent Resume

`agent resume` continues a persisted session. The CLI replays prior session
messages into the next model request and appends only the new turn's messages
and events back to storage. Resumed runs use the same default read-only tool
registry unless `--tool-set` is passed again.

```bash
CODEL00P_PROVIDER_CUSTOM_API_KEY=local-dev-key \
codel00p \
  --memory-db .codel00p/memory.sqlite \
  --organization-id org-1 \
  --project-id project-1 \
  --project-name codel00p \
  agent resume session-architecture "Continue with the next implementation step." \
  --workspace . \
  --provider custom \
  --model local-model \
  --base-url http://127.0.0.1:11434/v1
```

## Memory Review

```bash
codel00p ... memory similar --kind workflow --content "Run pnpm verify before pushing to main branch."
codel00p ... memory similar --kind workflow --content "Run pnpm verify before pushing to main branch." --json

codel00p ... memory search --text verify --kind workflow --tag verify
codel00p ... memory search --text verify --kind workflow --tag verify --json

codel00p ... memory list --status candidate
codel00p ... memory list --status candidate --json

codel00p ... memory show mem-1
codel00p ... memory show mem-1 --json
codel00p ... memory approve mem-1 --actor alice
codel00p ... memory approve mem-1 --actor alice --json
codel00p ... memory reject mem-1 --actor alice --reason "too vague"
codel00p ... memory archive mem-1 --actor alice --reason "obsolete"
codel00p ... memory edit mem-1 --actor alice --content "Run pnpm verify before pushing main." --json
codel00p ... memory audit mem-1
codel00p ... memory audit mem-1 --json
codel00p ... memory restore mem-1 --sequence 3 --actor alice --reason "undo edit"
codel00p ... memory restore mem-1 --sequence 3 --actor alice --reason "undo edit" --json
```

Output is intentionally stable and scriptable:

- `memory list` prints `id`, `status`, `kind`, and `content` as tab-separated
  fields; add `--json` for MCP-compatible record objects.
- `memory similar` prints active near-duplicate candidates as `id`, `status`,
  `kind`, `score`, and `content`; add `--json` for record objects with scores.
  The MCP `memory_similar` tool returns the same scored record objects.
- `memory search` prints approved memory as `id`, `status`, `kind`, `reason`,
  and `content`; add `--json` for MCP-compatible records with reasons.
- `memory show` prints a single memory record with source evidence; add
  `--json` for the MCP-compatible record object. Memory detail text and JSON
  records include `source_uri` when source evidence is available.
- review commands print `id` and the resulting status; add `--json` for the
  MCP-compatible record object.
- `memory edit` replaces content and prints `id` plus resulting status; add
  `--json` for the MCP-compatible record object.
- `memory audit` prints `sequence`, `action`, `actor`, and `reason`; add
  `--json` for machine-readable revision metadata including `memory_id`.
- `memory restore` restores content from an edit audit event's
  `previous_content` and prints `id` plus resulting status; add `--json` for
  the MCP-compatible record object.

## Session Replay

```bash
codel00p ... session show session-architecture
```

`session show` prints persisted records in tab-separated form:

```text
1	message	user	Inspect this repository.
2	message	assistant	Repository summary...
3	event	turn_started
```

## Memory Growth

After a completed agent turn, assistant responses can create candidate memory
with explicit directives:

```text
remember workflow[verify]: Run pnpm verify before pushing main.
```

Candidates are not injected into future turns until reviewed:

```bash
codel00p ... memory list --status candidate
codel00p ... memory approve memory-candidate-session-1-turn-1-1 --actor alice
```
