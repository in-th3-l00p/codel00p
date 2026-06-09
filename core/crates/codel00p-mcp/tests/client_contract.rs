use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use codel00p_harness::{Tool, ToolRegistry, Workspace};
use codel00p_mcp::{
    McpClient, McpClientNotification, McpResourceDescriptor, McpTool, McpToolCall,
    McpToolDescriptor, McpToolOutput, discover_tool_registry,
};
use codel00p_protocol::PermissionScope;
use serde_json::json;

#[test]
fn tool_descriptor_names_are_stable_and_prefixed() {
    let descriptor = McpToolDescriptor::new(
        "linear",
        "create_issue",
        "Create a Linear issue.",
        json!({ "type": "object" }),
    );

    assert_eq!(descriptor.server_id(), "linear");
    assert_eq!(descriptor.tool_name(), "create_issue");
    assert_eq!(descriptor.harness_tool_name(), "mcp.linear.create_issue");
    assert_eq!(
        descriptor.permission_scope(),
        PermissionScope::ExternalConnector
    );
}

#[test]
fn resource_descriptor_carries_server_uri_and_mime_type() {
    let resource = McpResourceDescriptor::new(
        "docs",
        "file:///workspace/README.md",
        "README",
        Some("text/markdown"),
    );

    assert_eq!(resource.server_id(), "docs");
    assert_eq!(resource.uri(), "file:///workspace/README.md");
    assert_eq!(resource.name(), "README");
    assert_eq!(resource.mime_type(), Some("text/markdown"));
}

#[tokio::test]
async fn mcp_tool_delegates_calls_to_client() {
    let workspace = Workspace::new(".").expect("workspace");
    let client = RecordingMcpClient::default();
    let descriptor = McpToolDescriptor::new(
        "docs",
        "search",
        "Search docs.",
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            }
        }),
    )
    .with_permission_scope(PermissionScope::ReadOnly);
    let tool = McpTool::new(descriptor.clone(), client.clone());

    assert_eq!(tool.name(), "mcp.docs.search");
    assert_eq!(tool.description(), "Search docs.");
    assert_eq!(tool.input_schema()["properties"]["query"]["type"], "string");
    assert_eq!(
        tool.permission_scope(&json!({ "query": "memory" })),
        PermissionScope::ReadOnly
    );

    let result = tool
        .execute(&workspace, json!({ "query": "memory" }))
        .await
        .expect("execute mcp tool");

    assert_eq!(
        client.calls(),
        vec![McpToolCall::new(
            "docs",
            "search",
            json!({ "query": "memory" }),
        )]
    );
    assert_eq!(result.content(), &json!({ "matches": ["memory.md"] }));
}

#[tokio::test]
async fn mcp_tool_maps_client_notifications_to_tool_progress() {
    let workspace = Workspace::new(".").expect("workspace");
    let client = RecordingMcpClient::default();
    client.set_output(
        McpToolOutput::json(json!({ "matches": ["memory.md"] })).with_notifications(vec![
            McpClientNotification::progress(json!("p1"), 1.0, Some(2.0), Some("Searching")),
            McpClientNotification::resource_updated("codel00p://memory/mem-1"),
            McpClientNotification::tools_list_changed(),
            McpClientNotification::resources_list_changed(),
        ]),
    );
    let tool = McpTool::new(
        McpToolDescriptor::new("docs", "search", "Search docs.", json!({})),
        client,
    );

    let result = tool
        .execute(&workspace, json!({ "query": "memory" }))
        .await
        .expect("execute mcp tool");

    assert_eq!(result.content(), &json!({ "matches": ["memory.md"] }));
    assert_eq!(result.progress()[0].phase(), "mcp_progress");
    assert_eq!(result.progress()[0].message(), Some("Searching"));
    assert_eq!(result.progress()[1].phase(), "mcp_resource_updated");
    assert_eq!(
        result.progress()[1].message(),
        Some("codel00p://memory/mem-1")
    );
    assert_eq!(result.progress()[2].phase(), "mcp_tools_list_changed");
    assert_eq!(result.progress()[2].message(), None);
    assert_eq!(result.progress()[3].phase(), "mcp_resources_list_changed");
    assert_eq!(result.progress()[3].message(), None);
}

#[tokio::test]
async fn mcp_tools_can_be_registered_in_harness_registry() {
    let workspace = Workspace::new(".").expect("workspace");
    let client = RecordingMcpClient::default();
    let registry = ToolRegistry::new().with_tool(McpTool::new(
        McpToolDescriptor::new("linear", "create_issue", "Create issue.", json!({})),
        client,
    ));

    assert_eq!(
        registry.names(),
        vec!["mcp.linear.create_issue".to_string()]
    );
    let result = registry
        .execute(
            "mcp.linear.create_issue",
            &workspace,
            json!({ "title": "Ship MCP" }),
        )
        .await
        .expect("execute mcp tool");

    assert_eq!(result.content(), &json!({ "matches": ["memory.md"] }));
}

#[tokio::test]
async fn discovers_mcp_tools_into_harness_registry() {
    let workspace = Workspace::new(".").expect("workspace");
    let client = RecordingMcpClient::with_tools(vec![
        McpToolDescriptor::new("linear", "create_issue", "Create issue.", json!({})),
        McpToolDescriptor::new("docs", "search", "Search docs.", json!({})),
    ]);

    let registry = discover_tool_registry(client.clone())
        .await
        .expect("discover tools");

    assert_eq!(
        registry.names(),
        vec![
            "mcp.docs.search".to_string(),
            "mcp.linear.create_issue".to_string(),
        ]
    );
    registry
        .execute("mcp.docs.search", &workspace, json!({ "query": "memory" }))
        .await
        .expect("execute discovered tool");
    assert_eq!(client.calls()[0].tool_name(), "search");
}

#[derive(Clone)]
struct RecordingMcpClient {
    calls: Arc<Mutex<Vec<McpToolCall>>>,
    output: Arc<Mutex<McpToolOutput>>,
    tools: Vec<McpToolDescriptor>,
}

impl Default for RecordingMcpClient {
    fn default() -> Self {
        Self::with_tools(Vec::new())
    }
}

impl RecordingMcpClient {
    fn with_tools(tools: Vec<McpToolDescriptor>) -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            output: Arc::new(Mutex::new(McpToolOutput::json(json!({
                "matches": ["memory.md"]
            })))),
            tools,
        }
    }

    fn calls(&self) -> Vec<McpToolCall> {
        self.calls.lock().expect("calls").clone()
    }

    fn set_output(&self, output: McpToolOutput) {
        *self.output.lock().expect("output") = output;
    }
}

#[async_trait]
impl McpClient for RecordingMcpClient {
    async fn call_tool(&self, call: McpToolCall) -> Result<McpToolOutput, codel00p_mcp::McpError> {
        self.calls.lock().expect("calls").push(call);
        Ok(self.output.lock().expect("output").clone())
    }

    async fn list_tools(&self) -> Result<Vec<McpToolDescriptor>, codel00p_mcp::McpError> {
        Ok(self.tools.clone())
    }

    async fn list_resources(&self) -> Result<Vec<McpResourceDescriptor>, codel00p_mcp::McpError> {
        Ok(Vec::new())
    }
}
