# codel00p-mcp

Transport-neutral MCP integration layer for codel00p.

This crate owns the contract between MCP servers and the agent harness:

- `McpClient`: async client trait for listing tools/resources and calling tools;
- `McpToolDescriptor`: server-provided tool metadata;
- `McpResourceDescriptor`: server-provided resource metadata;
- `McpTool`: adapter that exposes an MCP tool as a `codel00p-harness` tool;
- `discover_tool_registry`: builds a harness `ToolRegistry` from `list_tools`.
- stdio JSON-RPC line encoding/decoding helpers.
- `McpStdioClient`: process-backed stdio JSON-RPC client for MCP servers.
- `McpServerRuntime`: reusable server-side JSON-RPC response/error framing,
  progress notifications, resource subscriptions, and resource update
  notifications.
- `McpServerHandler` and `serve_stdio_server`: typed server dispatch and
  newline-delimited stdio serving for codel00p MCP servers.
- `McpClientNotification`: progress, resource-update, and list-changed
  notification metadata collected by MCP clients and emitted through harness
  `ToolProgress` events.

Harness tool names are prefixed as:

```text
mcp.<server_id>.<tool_name>
```

This keeps external tools visibly separate from native workspace, shell, and git
tools. MCP tools default to `PermissionScope::ExternalConnector`; descriptors
can opt into stricter or weaker scopes when the server/tool semantics are known.
The CLI uses this to let workspace config mark trusted MCP tools as
`read_only`, `workspace_write`, or `shell` when the default external connector
classification is too broad.

The stdio transport launches a configured server process, writes newline
delimited JSON-RPC messages to stdin, and reads newline delimited JSON-RPC
responses from stdout. Requests have a configurable timeout, and shutdown closes
server stdin, waits for process exit, then kills the server if it does not stop
in time. It can subscribe/unsubscribe resource URIs and poll subsequent
server-sent notifications from the same long-lived process.

The HTTP transport sends each JSON-RPC client message as a POST to the MCP
endpoint, accepts JSON or `text/event-stream` responses, stores
`Mcp-Session-Id` returned by `initialize`, and sends that session header on
later requests. It supports bearer tokens and static headers for enterprise
connector gateways. Stdio and HTTP tool calls preserve
`notifications/progress`, `notifications/resources/updated`, and list-changed
messages that arrive before the final response; the harness adapter maps them
into `mcp_progress`, `mcp_resource_updated`, `mcp_tools_list_changed`, and
`mcp_resources_list_changed` tool progress phases.

Both transports support the MCP lifecycle handshake by sending `initialize`,
recording the negotiated server metadata, and then sending
`notifications/initialized` before normal operation. Both map `tools/list`,
`resources/list`, and `tools/call` into codel00p descriptors and outputs.

The server runtime is deliberately transport-neutral. Callers provide a typed
`McpServerHandler`, and the runtime handles protocol mechanics around the final
result: ignored client notifications, JSON-RPC errors, optional progress
notifications from `_meta.progressToken`, subscription bookkeeping for
`resources/subscribe` and `resources/unsubscribe`, and
`notifications/resources/updated` for subscribed resources changed by a
successful operation. `serve_stdio_server` wraps that runtime in a reusable
newline-delimited stdio loop so product-specific servers only implement method
dispatch.
