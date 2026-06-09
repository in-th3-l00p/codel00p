# MCP Compatibility

codel00p MCP support targets the 2025-06-18 MCP shape and keeps all external
connectors behind the same harness tool, permission, event, and audit contracts
as native workspace tools.

## Supported Baseline

| Area | Status | Repository coverage |
| --- | --- | --- |
| Lifecycle | `initialize` plus `notifications/initialized` for stdio and HTTP clients | `core/crates/codel00p-mcp/tests/stdio_process_client.rs`, `core/crates/codel00p-mcp/tests/http_client.rs` |
| Tool discovery | `tools/list`, descriptor parsing, stable `mcp.<server>.<tool>` names, cursor pagination | `stdio_client_paginates_list_methods_until_cursor_is_absent`, `http_client_paginates_tool_lists_until_cursor_is_absent` |
| Tool calls | `tools/call` over stdio and HTTP, JSON and SSE responses, pre-response progress/resource notifications | `stdio_client_lists_and_calls_mcp_tools`, `http_client_calls_tools_over_json_rpc_post`, `http_client_collects_sse_notifications_before_tool_response` |
| Resources | `resources/list`, `resources/read`, text/blob content, cursor pagination | `stdio_client_reads_resources_templates_and_sets_logging_level`, `stdio_client_paginates_list_methods_until_cursor_is_absent` |
| Resource templates | `resources/templates/list`, template descriptors, cursor pagination | `stdio_client_reads_resources_templates_and_sets_logging_level`, `stdio_client_paginates_list_methods_until_cursor_is_absent` |
| Prompts | `prompts/list`, `prompts/get`, prompt arguments, cursor pagination | `stdio_client_lists_and_gets_mcp_prompts`, `stdio_client_paginates_list_methods_until_cursor_is_absent` |
| Logging | `logging/setLevel` and `notifications/message` mapping | `stdio_client_maps_logging_and_prompt_change_notifications`, `http_client_supports_prompts_resources_and_logging` |
| Roots | stdio client responses to server-originated `roots/list` | `stdio_client_answers_roots_list_requests_during_tool_discovery` |
| Subscriptions | `resources/subscribe`, `resources/unsubscribe`, resource/list-change/logging notification reads | `stdio_client_reads_notifications_after_resource_subscription` |
| Reconnects | bounded stdio reconnects, resource resubscribe, stable subscription events | `notification_supervisor_reconnects_stdio_servers_and_resubscribes` |
| Harness routing | MCP notifications and subscription states become stable `ToolProgress` events | `core/crates/codel00p-mcp/tests/client_contract.rs` |
| Server runtime | JSON-RPC framing, errors, progress, subscriptions, stdio serving | `core/crates/codel00p-mcp/tests/server_runtime.rs` |
| CLI diagnostics | `agent mcp doctor` validates configured servers and redacts secrets | `core/crates/codel00p-cli/tests/agent_cli.rs` |

## Third-Party Certification Targets

The repository has deterministic compatibility fixtures. Live third-party
certification should be tracked as separate fixture records so failures become
regression tests instead of tribal knowledge.

Initial certification targets:

- filesystem and git-oriented MCP servers;
- issue tracker servers such as GitHub, Linear, and Jira;
- database servers such as Postgres and SQLite gateways;
- browser/design servers such as Playwright, Figma, or screenshot tools;
- observability servers such as Sentry, Datadog, and Grafana;
- internal enterprise gateways that expose private tools over HTTP MCP.

Each certification entry should capture:

- server name, version, transport, and launch/config shape;
- supported capabilities observed during `agent mcp doctor`;
- tool/resource/prompt count;
- any auth, session, or pagination behavior;
- a minimal local regression fixture for every discovered protocol edge case.

## Operational Rules

- Run `codel00p agent mcp doctor` before trusting a connector in a workspace.
- Keep secrets in env/header configuration; diagnostic output must stay redacted.
- Prefer explicit permission scopes for high-impact tools instead of relying on
  the default `ExternalConnector` classification.
- When a third-party server exposes unusual JSON, cursor, SSE, or notification
  behavior, add a deterministic fixture under `core/crates/codel00p-mcp/tests`
  before changing client code.
