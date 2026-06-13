use super::{
    diagnostics::{diagnose_mcp_server, list_mcp_tools_for_server},
    spec::{McpServerSpec, load_mcp_servers_from_workspace, parse_mcp_server},
    *,
};

pub(crate) fn agent_mcp(_config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("missing agent mcp command".to_string());
    };
    match command.as_str() {
        "list" => agent_mcp_list(rest),
        "doctor" => agent_mcp_doctor(rest),
        _ => Err(format!("unknown agent mcp command: {command}")),
    }
}

fn agent_mcp_list(args: &[String]) -> CliResult<String> {
    let options = parse_agent_mcp_list_options(args)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to start async runtime: {error}"))?;

    runtime.block_on(async move {
        let mut servers = load_mcp_servers_from_workspace(&options.workspace)?;
        servers.extend(options.mcp_servers);
        let mut lines = Vec::new();
        for server in servers {
            for tool in list_mcp_tools_for_server(&server)
                .await
                .map_err(|error| error.to_string())?
            {
                lines.push(format!(
                    "{}\t{}",
                    tool.harness_tool_name(),
                    tool.description()
                ));
            }
        }
        lines.sort();
        Ok(if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        })
    })
}

fn agent_mcp_doctor(args: &[String]) -> CliResult<String> {
    let options = parse_agent_mcp_list_options(args)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to start async runtime: {error}"))?;

    runtime.block_on(async move {
        let mut servers = load_mcp_servers_from_workspace(&options.workspace)?;
        servers.extend(options.mcp_servers);
        let mut lines = Vec::new();
        for server in servers {
            lines.push(diagnose_mcp_server(&server).await);
        }
        lines.sort();
        Ok(if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        })
    })
}

struct AgentMcpListOptions {
    workspace: PathBuf,
    mcp_servers: Vec<McpServerSpec>,
}

fn parse_agent_mcp_list_options(args: &[String]) -> CliResult<AgentMcpListOptions> {
    let mut workspace = env::current_dir().map_err(|error| error.to_string())?;
    let mut mcp_servers = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--workspace" => {
                workspace = PathBuf::from(required_value(args, index, "--workspace")?);
                index += 2;
            }
            "--mcp-server" => {
                let value = required_value(args, index, "--mcp-server")?;
                mcp_servers.push(parse_mcp_server(&value)?);
                index += 2;
            }
            flag => return Err(format!("unknown agent mcp list option: {flag}")),
        }
    }
    Ok(AgentMcpListOptions {
        workspace,
        mcp_servers,
    })
}
