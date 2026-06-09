# codel00p-mcp

Transport-neutral MCP integration layer for codel00p.

This crate owns the contract between MCP servers and the agent harness:

- `McpClient`: async client trait for listing tools/resources and calling tools;
- `McpToolDescriptor`: server-provided tool metadata;
- `McpResourceDescriptor`: server-provided resource metadata;
- `McpTool`: adapter that exposes an MCP tool as a `codel00p-harness` tool;
- `discover_tool_registry`: builds a harness `ToolRegistry` from `list_tools`.
- stdio JSON-RPC line encoding/decoding helpers.

Harness tool names are prefixed as:

```text
mcp.<server_id>.<tool_name>
```

This keeps external tools visibly separate from native workspace, shell, and git
tools. MCP tools default to `PermissionScope::ExternalConnector`; descriptors
can opt into stricter or weaker scopes when the server/tool semantics are known.

The first implementation intentionally does not own stdio, HTTP, OAuth, or
server lifecycle. It does include the stdio message codec: MCP stdio messages
are UTF-8 JSON-RPC messages delimited by newlines, with no embedded newlines.
Process management and full initialization/list/call flows should plug into
`McpClient` so CLI, desktop, and cloud runtimes share one permission and
tool-execution contract.
