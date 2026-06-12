# Configuration

codel00p reads configuration from TOML files so commands run without repeating
flags. After a one-time `codel00p config setup`, most commands need no arguments.

## Files

```text
~/.codel00p/config.toml   user configuration
~/.codel00p/.env          provider API keys (chmod 600, never committed)
~/.codel00p/memory.sqlite default memory + session store
./.codel00p/config.toml   per-project configuration (optional)
```

The codel00p home directory is `~/.codel00p` by default; override it with the
`CODEL00P_HOME` environment variable.

## Precedence

Lowest to highest:

```text
built-in defaults
  < ~/.codel00p/config.toml        (user)
  < ./.codel00p/config.toml        (project, discovered by walking up from cwd)
  < CODEL00P_* environment vars
  < CLI flags (--provider, --model, --memory-db, ...)
```

CLI flags always win, so anything in the config can be overridden for a single
invocation. The user config directory is never treated as a project layer.

## Getting started

```bash
codel00p config setup                       # guided: provider, key, model
# or, explicitly:
codel00p providers use openrouter --model openai/gpt-4o-mini
codel00p providers set-key openrouter       # prompts for the key
codel00p agent chat                         # no flags needed
```

## `codel00p config`

| Command | Description |
| --- | --- |
| `config show [--json\|--raw]` | Show the effective configuration (default) |
| `config setup` | Guided first-run setup |
| `config path [--project]` | Print the config file path |
| `config edit [--project]` | Open the config file in `$EDITOR` |
| `config init [--force]` | Write a starter config file |
| `config reset` | Restore the user config to defaults |
| `config migrate` | Upgrade the config to the current schema version |
| `config get <key>` | (advanced) print one value |
| `config set <key> <value>` | (advanced) set one value |
| `config unset <key>` | (advanced) remove one value |

Add `--project` to `set`/`unset`/`init`/`edit`/`path` to target
`./.codel00p/config.toml` instead of the user config.

### Keys

```text
workspace.organization_id      workspace.project_id
workspace.project_name         workspace.memory_db
agent.provider                 agent.model
agent.base_url                 agent.provider_policy_preset
agent.max_iterations           agent.permission_mode      # allow | ask | deny
agent.tool_sets                # comma-separated: read,edit,command,git,delegate,all
agent.stream                   agent.remember_permissions
plugins.enabled                # comma-separated plugin ids (see `codel00p plugins`)
delegation.max_concurrent_children   # cap on child agents run concurrently (default 4)
```

The `delegate` tool-set (`--tool-set delegate` or `agent.tool_sets`) lets an
agent hand focused tasks to child agents via a `delegate_task` tool. Children run
read-only and are recorded as their own sessions linked to the parent.

## `codel00p plugins`

Plugins contribute extra tools and lifecycle hooks to agent runs. Enabling a
plugin records its id in `[plugins] enabled`; the agent loads enabled plugins
from the built-in catalog at the start of each run.

| Command | Description |
| --- | --- |
| `plugins list` | List available plugins and their enabled state (default) |
| `plugins enable <id> [--project]` | Enable a plugin (user config, or project) |
| `plugins disable <id> [--project]` | Disable a plugin |

Enabling is an allow-list of built-in ids, not arbitrary code loading. An enabled
id the catalog no longer knows is skipped with a warning rather than failing the
run.

## `codel00p providers`

| Command | Description |
| --- | --- |
| `providers list` | List providers and credential status (default) |
| `providers use <id> [--model <m>] [--base-url <url>] [--preset <id>] [--project]` | Set the default provider/model |
| `providers set-key <id> [<key>]` | Store an API key in `~/.codel00p/.env` (prompts if omitted) |
| `providers remove-key <id>` | Remove a stored API key |
| `providers show <id>` | Show details for one provider |

API keys are written to `~/.codel00p/.env` (and loaded at startup), never to
`config.toml`. Environment variables already set in the shell take precedence
over the `.env` file.

## Example `config.toml`

```toml
config_version = 1

[workspace]
organization_id = "acme"
project_id = "web"
project_name = "Web App"

[agent]
provider = "openrouter"
model = "openai/gpt-4o-mini"
stream = true
permission_mode = "ask"
tool_sets = ["read", "edit"]

[plugins]
enabled = ["system-info"]
```
