# codel00p-cli

Terminal interface for codel00p.

The current CLI is the first usable codel00p product slice: a read-only coding
agent with durable sessions and reviewed project memory.

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

`agent run` executes one read-only harness turn against a workspace. It supports
OpenAI-compatible Chat Completions providers first: GitHub Copilot/GitHub
Models, OpenRouter, Azure AI Foundry compatible endpoints, and custom/local
OpenAI-compatible endpoints.

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

Provider credentials are read from environment variables:

- GitHub/Copilot: `CODEL00P_PROVIDER_GITHUB_TOKEN`, `COPILOT_GITHUB_TOKEN`,
  `GH_TOKEN`, `GITHUB_TOKEN`.
- OpenRouter: `CODEL00P_PROVIDER_OPENROUTER_API_KEY`, `OPENROUTER_API_KEY`.
- Azure AI Foundry compatible: `CODEL00P_PROVIDER_AZURE_FOUNDRY_API_KEY`,
  `AZURE_FOUNDRY_API_KEY`.
- Custom/local OpenAI-compatible: `CODEL00P_PROVIDER_CUSTOM_API_KEY`.

`openai`, `anthropic`, `bedrock`, and native `gemini` registry entries exist in
the provider layer, but `agent run` currently rejects non-Chat-Completions modes
until their transports are implemented.

## Memory Review

```bash
codel00p ... memory list --status candidate

codel00p ... memory show mem-1
codel00p ... memory approve mem-1 --actor alice
codel00p ... memory reject mem-1 --actor alice --reason "too vague"
codel00p ... memory archive mem-1 --actor alice --reason "obsolete"
codel00p ... memory audit mem-1
```

Output is intentionally stable and scriptable:

- `memory list` prints `id`, `status`, `kind`, and `content` as tab-separated
  fields.
- review commands print `id` and the resulting status.
- `memory audit` prints `sequence`, `action`, `actor`, and `reason`.

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
