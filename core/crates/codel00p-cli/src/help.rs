pub fn help_for(args: &[String]) -> Option<&'static str> {
    match args {
        [] => None,
        [flag] if is_help(flag) => Some(TOP_LEVEL_HELP),
        [command, flag] if is_help(flag) => match command.as_str() {
            "agent" => Some(AGENT_HELP),
            "config" => Some(CONFIG_HELP),
            "auth" => Some(AUTH_HELP),
            "skills" => Some(SKILLS_HELP),
            "cron" => Some(CRON_HELP),
            "gateway" => Some(GATEWAY_HELP),
            "mcp" => Some(MCP_HELP),
            "memory" => Some(MEMORY_HELP),
            "session" => Some(SESSION_HELP),
            "cloud" => Some(CLOUD_HELP),
            "update" => Some(UPDATE_HELP),
            "uninstall" => Some(UNINSTALL_HELP),
            _ => None,
        },
        [command, subcommand, flag] if is_help(flag) => {
            match (command.as_str(), subcommand.as_str()) {
                ("agent", "run") => Some(AGENT_RUN_HELP),
                ("agent", "resume") => Some(AGENT_RESUME_HELP),
                ("agent", "continue") => Some(AGENT_CONTINUE_HELP),
                ("agent", "chat") => Some(AGENT_CHAT_HELP),
                ("agent", "mcp") => Some(AGENT_MCP_HELP),
                ("config", "providers") => Some(PROVIDERS_HELP),
                ("config", "plugins") => Some(PLUGINS_HELP),
                ("auth", "login") => Some(LOGIN_HELP),
                ("auth", "logout") => Some(LOGOUT_HELP),
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
  codel00p — your terminal coding agent

Usage
  codel00p [options] [command]
  codel00p                       open the interactive chat (default)

Commands
  agent      Run the agent — run · resume · continue · chat · mcp
  config     Settings, providers, and plugins
  auth       Sign in or out of the codel00p cloud
  cloud      Sync project memory with your team
  session    Inspect persisted sessions
  memory     Review project memory
  skills     List, show, and scaffold skills
  cron       Schedule unattended agent jobs
  gateway    Reach the agent from chat platforms
  mcp        Expose codel00p as an MCP server
  update     Check for and install a newer codel00p
  uninstall  Remove codel00p from this machine
  version    Print the installed version

Options
  --project-id <id>          Project scope
  --organization-id <id>     Organization scope
  --project-name <name>      Project display name
  --memory-db <path>         Memory and session database

  Run `codel00p <command> --help` for details · `codel00p config setup` to begin.
";

const UPDATE_HELP: &str = "\
Usage: codel00p update [options]

Checks GitHub Releases for a newer codel00p and installs it in place, replacing the
running binary. codel00p also checks for updates in the background (at most once a
day) and nudges you when a newer release is available.

Options:
  --check            Report whether an update is available; do not install
  --yes, -y          Install without the confirmation prompt
  --force            Reinstall even if already up to date
  --version <tag>    Install a specific release tag, e.g. v0.2.0

Environment:
  CODEL00P_DISABLE_UPDATE_CHECK   set to any value to silence background checks
";

const UNINSTALL_HELP: &str = "\
Usage: codel00p uninstall [options]

Removes codel00p from this machine. By default it deletes the installed binary
and keeps your data so a reinstall picks up where you left off. It shows what
will be removed and asks for confirmation first.

Options:
  --purge            Also delete ~/.codel00p (config, credentials, sessions, memory)
  --yes, -y          Skip the confirmation prompt (required in non-interactive shells)

Notes:
  • On Windows the running binary cannot delete itself; the path is printed for
    manual removal.
  • If you added the install directory to your shell PATH, remove that line too.
";

const AUTH_HELP: &str = "\
Usage: codel00p auth <command>

Sign in to the codel00p cloud so `codel00p cloud` commands work without a token.

Commands:
  login    Sign in via your browser; stores a session token
  logout   Clear stored cloud credentials
";

const LOGIN_HELP: &str = "\
Usage: codel00p auth login [options]

Sign in to the codel00p cloud. Opens your browser to authenticate (OAuth or
email code via Clerk), then stores a session token in
~/.codel00p/credentials.toml so `codel00p cloud` commands work without --token.

Options:
  --api-url <url>      Cloud service URL to store with the credentials
                       (or set CODEL00P_API_URL)
  --connect-url <url>  Web sign-in handoff URL
                       (default http://localhost:3000/connect/cli,
                        or set CODEL00P_LOGIN_URL)
  --org <id>           Request a token scoped to a Clerk organization

Run `codel00p auth logout` to clear stored credentials.
";

const LOGOUT_HELP: &str = "\
Usage: codel00p auth logout

Clear the cloud credentials stored in ~/.codel00p/credentials.toml.
";

const CLOUD_HELP: &str = "\
Usage: codel00p cloud <command> [options]

Sync project memory with the codel00p cloud service. The org comes from the
session token; the cloud project is selected with --project.

Connection options (or env CODEL00P_API_URL / CODEL00P_TOKEN / CODEL00P_CLOUD_PROJECT):
  --api-url <url>    Base URL of the codel00p-cloud service
  --token <jwt>      Clerk session token for authentication
  --project <id>     Cloud project id

Commands:
  status   Show the authenticated viewer and active organization
  push     Push local memory (default: approved) to the project review queue
  pull     Import approved cloud memory into the local store
  run      Resolve and run a stored cloud agent (config + MCP + RAG context)

Push options:  --status <status>  --limit <n>  --dry-run  --json
Pull options:  --actor <name>  --json
Run usage:     codel00p cloud run <agent-id> --task \"...\" [--plan] [--limit n] [--json]
";

const GATEWAY_HELP: &str = "\
Usage: codel00p gateway <command>

Reach one agent core from chat platforms. Each conversation maps to a durable
agent session, so a thread is a continuous, resumable conversation. A platform
adapter (Slack, Telegram, a webhook) calls `gateway message` per inbound message.

Commands:
  message --conversation <id> --user <id> <text>
                                Handle one inbound message and print the reply.
                                Control text: /help, /stop, /approve, /deny.
  serve [--bind <addr>] [--port <n>]
                                Run an HTTP webhook (default 127.0.0.1:8765).
                                POST /message {conversation,user,text} -> {reply};
                                GET /healthz. Adapters post platform events here.

Messages run as restricted (read-only) agent turns for now.
";

const CRON_HELP: &str = "\
Usage: codel00p cron <command>

Define jobs that run a prompt on a schedule. Jobs are saved under
~/.codel00p/cron. Schedules are duration intervals: 30m, 2h, 1d, 1w (optionally
prefixed with `every`).

Commands:
  list                          List scheduled jobs (default)
  add <schedule> <prompt>       Add an agent job (--workspace/--provider/--model)
  add-command <schedule> <cmd>  Add a maintenance job that runs `codel00p <cmd>`
                                (e.g. add-command 1d skills curate --apply)
  show <id>                     Show a job's details
  remove <id>                   Delete a job
  enable <id> / disable <id>    Toggle a job on or off
  run <id>                      Run a job now
  daemon [--interval <dur>]     Run due jobs on a loop (default every 60s)
         [--once]               Run one check now and exit (e.g. from system cron)
";

const CONFIG_HELP: &str = "\
Usage: codel00p config <command>

Configuration lives in ~/.codel00p/config.toml (user) and ./.codel00p/config.toml
(project). Precedence: defaults < user < project < env vars < CLI flags.

Commands:
  show [--json|--raw]   Show the effective configuration (default)
  setup                 Guided first-run setup (provider, key, model)
  providers <command>   Inference providers and API keys (try `--help`)
  plugins <command>     Enable or disable agent plugins (try `--help`)
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
Usage: codel00p config providers <command>

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
  curate [--apply]              List (or archive) stale, unused agent-created
         [--min-age <dur>]      skills; reversible (default grace period 7d)
";

const PLUGINS_HELP: &str = "\
Usage: codel00p config plugins <command>

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
  run       Run one agent turn
  resume    Resume a persisted agent session
  continue  Resume the most recent session
  chat      Start an interactive multi-turn chat session
  mcp       Inspect MCP server tools
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

const AGENT_CONTINUE_HELP: &str = "\
Usage: codel00p [global options] agent continue <prompt> [options]

Resumes the most recently created session, so you can keep going without
looking up its id. Accepts the same options as `agent run`.

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
