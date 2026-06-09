pub fn help_for(args: &[String]) -> Option<&'static str> {
    match args {
        [] => None,
        [flag] if is_help(flag) => Some(TOP_LEVEL_HELP),
        [command, flag] if is_help(flag) => match command.as_str() {
            "agent" => Some(AGENT_HELP),
            "memory" => Some(MEMORY_HELP),
            "session" => Some(SESSION_HELP),
            _ => None,
        },
        [command, subcommand, flag] if is_help(flag) => {
            match (command.as_str(), subcommand.as_str()) {
                ("agent", "run") => Some(AGENT_RUN_HELP),
                ("agent", "resume") => Some(AGENT_RESUME_HELP),
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
  memory   Review project memory
  session  Inspect persisted sessions
";

const AGENT_HELP: &str = "\
Usage: codel00p [global options] agent <command>

Commands:
  run      Run one agent turn
  resume   Resume a persisted agent session
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
  --stream-events             Stream serialized harness events during the turn
  --json-events               Print serialized harness events after assistant text
";

const MEMORY_HELP: &str = "\
Usage: codel00p [global options] memory <command>

Commands:
  list     List memory records
  show     Show one memory record
  audit    Show memory audit history
  approve  Approve candidate memory
  reject   Reject candidate memory
  archive  Archive memory
";

const SESSION_HELP: &str = "\
Usage: codel00p [global options] session <command>

Commands:
  show     Show persisted session records
";
