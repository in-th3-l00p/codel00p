//! Local CLI commands for remembered MCP connector permissions.

use super::*;

pub(super) fn permissions(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("missing mcp permissions command".to_string());
    };

    match command.as_str() {
        "list" => permissions_list(&config, rest),
        "forget" => permissions_forget(&config, rest),
        _ => Err(format!("unknown mcp permissions command: {command}")),
    }
}

fn permissions_list(config: &CliConfig, args: &[String]) -> CliResult<String> {
    if !args.is_empty() {
        return Err("mcp permissions list does not accept arguments".to_string());
    }
    let mut output = String::new();
    for decision in list_decisions(config)? {
        output.push_str(&format!(
            "{}\t{}\t{}\n",
            decision.tool_name,
            scope_label(decision.scope),
            connector_status_label(decision.status)
        ));
    }
    Ok(output)
}

fn permissions_forget(config: &CliConfig, args: &[String]) -> CliResult<String> {
    let Some(tool_name) = args.first() else {
        return Err("mcp permissions forget expects a tool name".to_string());
    };
    let mut scope = "external_connector".to_string();
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--scope" => {
                scope = required_value(args, index, "--scope")?;
                index += 2;
            }
            value => return Err(format!("unknown mcp permissions forget option: {value}")),
        }
    }
    let scope = parse_scope_label(&scope)?;
    let status = if forget_decision(config, tool_name, scope)? {
        "forgot"
    } else {
        "missing"
    };
    Ok(format!("{status}\t{tool_name}\t{}\n", scope_label(scope)))
}
