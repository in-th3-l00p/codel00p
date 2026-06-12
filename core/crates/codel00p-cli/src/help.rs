pub fn help_for(args: &[String]) -> Option<&'static str> {
    match args {
        [] => None,
        [flag] if is_help(flag) => Some(TOP_LEVEL_HELP),
        [command, flag] if is_help(flag) => match command.as_str() {
            "agent" => Some(AGENT_HELP),
            "config" => Some(CONFIG_HELP),
            "providers" => Some(PROVIDERS_HELP),
            "plugins" => Some(PLUGINS_HELP),
            "skills" => Some(SKILLS_HELP),
            "cron" => Some(CRON_HELP),
            "mcp" => Some(MCP_HELP),
            "memory" => Some(MEMORY_HELP),
            "session" => Some(SESSION_HELP),
            _ => None,
        },
        [command, subcommand, flag] if is_help(flag) => {
            match (command.as_str(), subcommand.as_str()) {
                ("agent", "run") => Some(AGENT_RUN_HELP),
                ("agent", "resume") => Some(AGENT_RESUME_HELP),
                ("agent", "chat") => Some(AGENT_CHAT_HELP),
                ("agent", "mcp") => Some(AGENT_MCP_HELP),
                ("mcp", "permissions") => Some(MCP_PERMISSIONS_HELP),
                ("mcp", "serve") => Some(MCP_SERVE_HELP),
                ("session", "list") => Some(SESSION_LIST_HELP),
                _ => None,
            }
        }
        [command, subcommand, nested, flag] if is_help(flag) => {
            match (command.as_str(), subcommand.as_str(), nested.as_str()) {
                ("agent", "mcp", "list") => Some(AGENT_MCP_LIST_HELP),
                ("agent", "mcp", "doctor") => Some(AGENT_MCP_DOCTOR_HELP),
                ("mcp", "permissions", "list") => Some(MCP_PERMISSIONS_LIST_HELP),
                ("mcp", "permissions", "forget") => Some(MCP_PERMISSIONS_FORGET_HELP),
                _ => None,
            }
        }
        _ => None,
    }
}

fn is_help(value: &str) -> bool {
    matches!(value, "--help" | "-h" | "help")
}

const TOP_LEVEL_HELP: &str = "\
Usage: codel00p [global options] <command>

Configuration is read from ~/.codel00p/config.toml (and ./.codel00p/config.toml).
Run `codel00p config setup` once, then most commands need no flags.

Global options (override configuration for one invocation):
  --memory-db <path>          SQLite database for memory and sessions
  --organization-id <id>      Organization scope
  --project-id <id>           Project scope
  --project-name <name>       Project display name

Commands:
  agent      Run the coding agent
  config     View and edit configuration
  providers  Configure inference providers and credentials
  plugins    Enable or disable agent plugins
  skills     List, show, and scaffold skills
  cron       Define and manage scheduled jobs
  mcp        Expose codel00p as an MCP server
  memory     Review project memory
  session    Inspect persisted sessions
";

const CRON_HELP: &str = "\
Usage: codel00p cron <command>

Define jobs that run a prompt on a schedule. Jobs are saved under
~/.codel00p/cron. Schedules are duration intervals: 30m, 2h, 1d, 1w (optionally
prefixed with `every`). A background scheduler daemon is a later slice.

Commands:
  list                          List scheduled jobs (default)
  add <schedule> <prompt>       Add a job (--workspace/--provider/--model)
  show <id>                     Show a job's details
  remove <id>                   Delete a job
  enable <id> / disable <id>    Toggle a job on or off
  run <id>                      Run a job now as a read-only agent turn
";

const CONFIG_HELP: &str = "\
Usage: codel00p config <command>

Configuration lives in ~/.codel00p/config.toml (user) and ./.codel00p/config.toml
(project). Precedence: defaults < user < project < env vars < CLI flags.

Commands:
  show [--json|--raw]   Show the effective configuration (default)
  setup                 Guided first-run setup (provider, key, model)
  path [--project]      Print the config file path
  edit [--project]      Open the config file in $EDITOR
  init [--force]        Write a starter config file
  reset                 Restore the user config to defaults
  migrate               Upgrade the config to the current schema version

Advanced (raw key access):
  get <key>             Print one value, e.g. `config get agent.model`
  set <key> <value>     Set one value, e.g. `config set agent.stream true`
  unset <key>           Remove one value

Keys: workspace.{organization_id,project_id,project_name,memory_db},
      agent.{provider,model,base_url,provider_policy_preset,max_iterations,
      permission_mode,tool_sets,stream,remember_permissions}
";

const PROVIDERS_HELP: &str = "\
Usage: codel00p providers <command>

Configure which provider/model codel00p uses and store API keys (in
~/.codel00p/.env, never in config.toml).

Commands:
  list                          List providers and credential status (default)
  use <id> [--model <model>]    Set the default provider/model
              [--base-url <url>] [--preset <id>] [--project]
  set-key <id> [<key>]          Store an API key (prompts if omitted)
  remove-key <id>               Remove a stored API key
  show <id>                     Show details for one provider
";

const SKILLS_HELP: &str = "\
Usage: codel00p skills <command>

Skills are procedural memory: a SKILL.md (front matter + Markdown instructions)
in ~/.codel00p/skills (user) or ./.codel00p/skills (project). Project skills
override user skills with the same name. With `--tool-set learn`, an agent can
propose skills it learns; proposals wait in the review queue below and are not
used until approved.

Commands:
  list                          List active skills (default)
  show <name>                   Show a skill's metadata and instructions
  create <name> [--project]     Scaffold a new skill (user config, or project)
  candidates                    List agent-proposed skills awaiting review
  approve <name> [--project]    Approve a candidate (it becomes active)
  reject <name> [--project]     Reject a candidate (archived)
";

const PLUGINS_HELP: &str = "\
Usage: codel00p plugins <command>

Enable or disable plugins that add tools and lifecycle hooks to agent runs.
Enabled plugin ids are stored in [plugins] enabled in config.toml.

Commands:
  list                          List available plugins and enabled state (default)
  enable <id> [--project]       Enable a plugin (user config, or project)
  disable <id> [--project]      Disable a plugin
";

const AGENT_HELP: &str = "\
Usage: codel00p [global options] agent <command>

Commands:
  run      Run one agent turn
  resume   Resume a persisted agent session
  chat     Start an interactive multi-turn chat session
  mcp      Inspect MCP server tools
";

const AGENT_RUN_HELP: &str = "\
Usage: codel00p [global options] agent run <prompt> [options]

Options:
  --workspace <path>          Workspace root, defaults to current directory
  --provider <id>             Provider id or alias
  --model <id>                Provider model id
  --provider-policy-preset <id>
                              Built-in provider policy preset id
  --base-url <url>            Override provider base URL
  --session-id <id>           Persist under a stable session id
  --max-iterations <n>        Maximum model/tool iterations
  --tool-set <name>           Enable a tool set: read, edit, command, git, delegate, learn, all
  --mcp-server <id=command>   Attach an MCP stdio server executable
  --permission-mode <mode>    Tool permission mode: allow, ask, deny
  --remember-permissions      Persist ask-mode MCP connector decisions
  --stream-events             Stream serialized harness events during the turn
  --stream                    Stream assistant text token by token to stdout
  --json-events               Print serialized harness events after assistant text
";

const AGENT_RESUME_HELP: &str = "\
Usage: codel00p [global options] agent resume <session-id> <prompt> [options]

Options:
  --workspace <path>          Workspace root, defaults to current directory
  --provider <id>             Provider id or alias
  --model <id>                Provider model id
  --provider-policy-preset <id>
                              Built-in provider policy preset id
  --base-url <url>            Override provider base URL
  --max-iterations <n>        Maximum model/tool iterations
  --tool-set <name>           Enable a tool set: read, edit, command, git, delegate, learn, all
  --mcp-server <id=command>   Attach an MCP stdio server executable
  --permission-mode <mode>    Tool permission mode: allow, ask, deny
  --remember-permissions      Persist ask-mode MCP connector decisions
  --stream-events             Stream serialized harness events during the turn
  --stream                    Stream assistant text token by token to stdout
  --json-events               Print serialized harness events after assistant text
";

const AGENT_CHAT_HELP: &str = "\
Usage: codel00p [global options] agent chat [options]

Start an interactive multi-turn chat session. Conversation history is kept in
memory across turns and persisted under the session id; pass an existing
--session-id to resume a saved conversation. In-session commands:
  /help            Show available commands
  /session         Show the current session id
  /sessions        List all persisted conversations
  /history         Show the current conversation
  /tools           List the tools available this turn
  /model [id]      Show or switch the model for later turns
  /memory          Show approved project memory in context
  /reset           Start a new conversation
  /exit, /quit     Leave the chat

Options:
  --workspace <path>          Workspace root, defaults to current directory
  --provider <id>             Provider id or alias
  --model <id>                Provider model id
  --provider-policy-preset <id>
                              Built-in provider policy preset id
  --base-url <url>            Override provider base URL
  --session-id <id>           Persist under a stable session id
  --max-iterations <n>        Maximum model/tool iterations per turn
  --tool-set <name>           Enable a tool set: read, edit, command, git, delegate, learn, all
  --mcp-server <id=command>   Attach an MCP stdio server executable
  --permission-mode <mode>    Tool permission mode: allow, ask, deny
  --remember-permissions      Persist ask-mode MCP connector decisions
  --stream-events             Stream serialized harness events during each turn
  --stream                    Stream assistant text token by token to stdout
  --json-events               Print serialized harness events after each reply
";

const AGENT_MCP_HELP: &str = "\
Usage: codel00p [global options] agent mcp <command>

Commands:
  list     List configured MCP server tools
  doctor   Validate configured MCP servers and print redacted diagnostics
";

const AGENT_MCP_LIST_HELP: &str = "\
Usage: codel00p [global options] agent mcp list [options]

Options:
  --workspace <path>          Workspace root, defaults to current directory
  --mcp-server <id=command>   Attach an MCP stdio server executable
";

const AGENT_MCP_DOCTOR_HELP: &str = "\
Usage: codel00p [global options] agent mcp doctor [options]

Options:
  --workspace <path>          Workspace root, defaults to current directory
  --mcp-server <id=command>   Attach an MCP stdio server executable
";

const MCP_HELP: &str = "\
Usage: codel00p [global options] mcp <command>

Commands:
  serve        Run a stdio MCP server for codel00p memory and sessions
  permissions  Inspect remembered MCP connector permission decisions
";

const MCP_SERVE_HELP: &str = "\
Usage: codel00p [global options] mcp serve
";

const MCP_PERMISSIONS_HELP: &str = "\
Usage: codel00p [global options] mcp permissions <command>

Commands:
  list     List remembered MCP connector permission decisions
  forget   Forget one remembered MCP connector permission decision
";

const MCP_PERMISSIONS_LIST_HELP: &str = "\
Usage: codel00p [global options] mcp permissions list
";

const MCP_PERMISSIONS_FORGET_HELP: &str = "\
Usage: codel00p [global options] mcp permissions forget <tool-name> [options]

Options:
  --scope <scope>             Permission scope, defaults to external_connector
";

const MEMORY_HELP: &str = "\
Usage: codel00p [global options] memory <command>

Commands:
  similar  Score active near-duplicate memory; use --json for JSON output
  stale    List approved memory likely superseded by newer active memory
  quality  List active memory with low advisory quality scores
  search   Search approved memory records; supports --sensitivity and --json
  list     List memory records; supports --sensitivity and --json
  show     Show one memory record; use --json for JSON output
  audit    Show memory audit history; use --json for JSON output
  edit     Edit memory content; use --json for JSON output
  restore  Restore content from an edit audit sequence; use --json for JSON output
  approve  Approve candidate memory; use --json for JSON output
  reject   Reject candidate memory; use --json for JSON output
  archive  Archive memory; use --json for JSON output
";

const SESSION_LIST_HELP: &str = "\
Usage: codel00p [global options] session list [--json]

List every persisted conversation in the active project scope with its source,
message count, and event count.
";

const SESSION_HELP: &str = "\
Usage: codel00p [global options] session <command>

Commands:
  list     List persisted conversations; use --json for JSON output
  show     Show persisted session records
";
