//! stdio JSON-RPC server dispatch for the codel00p MCP surface.

use super::*;

pub(super) fn serve_stdio(config: CliConfig) -> CliResult<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut handler = Codel00pMcpServer { config };
    serve_stdio_server(stdin.lock(), stdout, &mut handler).map_err(|error| error.to_string())
}

struct Codel00pMcpServer {
    config: CliConfig,
}

impl McpServerHandler for Codel00pMcpServer {
    fn handle_method(&mut self, method: &str, params: &Value) -> Result<McpServerResponse, String> {
        dispatch_json_rpc(&self.config, method, params)
    }
}

fn dispatch_json_rpc(
    config: &CliConfig,
    method: &str,
    params: &Value,
) -> Result<McpServerResponse, String> {
    let result = match method {
        "initialize" => json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {
                "tools": {},
                "resources": {
                    "subscribe": true
                }
            },
            "serverInfo": {
                "name": "codel00p",
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
        "tools/list" => json!({ "tools": mcp_tools() }),
        "tools/call" => return call_tool(config, params),
        "resources/list" => json!({
            "resources": [],
            "resourceTemplates": mcp_resource_templates()
        }),
        "resources/read" => read_resource(config, params)?,
        _ => return Err(format!("unsupported method: {method}")),
    };
    Ok(McpServerResponse::new(result))
}
