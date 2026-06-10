pub fn help_for(args: &[String]) -> Option<&'static str> {
    match args {
        [] => None,
        [flag] if is_help(flag) => Some(TOP_LEVEL_HELP),
        [command, flag] if is_help(flag) => match command.as_str() {
            "agent" => Some(AGENT_HELP),
            "mcp" => Some(MCP_HELP),
            "memory" => Some(MEMORY_HELP),
            "session" => Some(SESSION_HELP),
            _ => None,
        },
        [command, subcommand, flag] if is_help(flag) => {
            match (command.as_str(), subcommand.as_str()) {
                ("agent", "run") => Some(AGENT_RUN_HELP),
                ("agent", "resume") => Some(AGENT_RESUME_HELP),
                ("agent", "mcp") => Some(AGENT_MCP_HELP),
                ("mcp", "permissions") => Some(MCP_PERMISSIONS_HELP),
                ("mcp", "serve") => Some(MCP_SERVE_HELP),
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

Global options:
  --memory-db <path>          SQLite database for memory and sessions
  --organization-id <id>      Organization scope
  --project-id <id>           Project scope
  --project-name <name>       Project display name

Commands:
  agent    Run the coding agent
  mcp      Expose codel00p as an MCP server
  memory   Review project memory
  session  Inspect persisted sessions
";

const AGENT_HELP: &str = "\
Usage: codel00p [global options] agent <command>

Commands:
  run      Run one agent turn
  resume   Resume a persisted agent session
  mcp      Inspect MCP server tools
";

const AGENT_RUN_HELP: &str = "\
Usage: codel00p [global options] agent run <prompt> [options]

Options:
  --workspace <path>          Workspace root, defaults to current directory
  --provider <id>             Provider id or alias
  --model <id>                Provider model id
  --base-url <url>            Override provider base URL
  --session-id <id>           Persist under a stable session id
  --max-iterations <n>        Maximum model/tool iterations
  --tool-set <name>           Enable a tool set: read, edit, command, git, all
  --mcp-server <id=command>   Attach an MCP stdio server executable
  --permission-mode <mode>    Tool permission mode: allow, ask, deny
  --remember-permissions      Persist ask-mode MCP connector decisions
  --stream-events             Stream serialized harness events during the turn
  --json-events               Print serialized harness events after assistant text
";

const AGENT_RESUME_HELP: &str = "\
Usage: codel00p [global options] agent resume <session-id> <prompt> [options]

Options:
  --workspace <path>          Workspace root, defaults to current directory
  --provider <id>             Provider id or alias
  --model <id>                Provider model id
  --base-url <url>            Override provider base URL
  --max-iterations <n>        Maximum model/tool iterations
  --tool-set <name>           Enable a tool set: read, edit, command, git, all
  --mcp-server <id=command>   Attach an MCP stdio server executable
  --permission-mode <mode>    Tool permission mode: allow, ask, deny
  --remember-permissions      Persist ask-mode MCP connector decisions
  --stream-events             Stream serialized harness events during the turn
  --json-events               Print serialized harness events after assistant text
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
  list     List memory records
  show     Show one memory record
  audit    Show memory audit history
  edit     Edit memory content
  approve  Approve candidate memory
  reject   Reject candidate memory
  archive  Archive memory
";

const SESSION_HELP: &str = "\
Usage: codel00p [global options] session <command>

Commands:
  show     Show persisted session records
";
