use async_trait::async_trait;
use codel00p_harness::{HarnessError, Tool, ToolRegistry, ToolResult, Workspace};
use serde_json::{Value, json};
use tempfile::tempdir;

struct EchoTool;

struct DynamicTool {
    name: String,
}

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Echoes the input payload."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "value": { "type": "string" }
            }
        })
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        Ok(ToolResult::json(json!({
            "echoed": input["value"],
        })))
    }
}

#[async_trait]
impl Tool for DynamicTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "Dynamic test tool."
    }

    fn input_schema(&self) -> Value {
        json!({ "type": "object" })
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        _input: Value,
    ) -> Result<ToolResult, HarnessError> {
        Ok(ToolResult::json(json!({ "tool": self.name })))
    }
}

#[tokio::test]
async fn registers_and_dispatches_tools_by_name() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::new().with_tool(EchoTool);

    let result = registry
        .execute("echo", &workspace, json!({ "value": "hello" }))
        .await
        .expect("execute tool");

    assert_eq!(result.content(), &json!({ "echoed": "hello" }));
}

#[test]
fn lists_tool_names_in_stable_order() {
    let registry = ToolRegistry::new().with_tool(EchoTool);

    assert_eq!(registry.names(), vec!["echo".to_string()]);
}

#[tokio::test]
async fn registers_dynamic_tool_names() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::new().with_tool(DynamicTool {
        name: "mcp.linear.create_issue".to_string(),
    });

    assert_eq!(
        registry.names(),
        vec!["mcp.linear.create_issue".to_string()]
    );
    let result = registry
        .execute("mcp.linear.create_issue", &workspace, json!({}))
        .await
        .expect("execute dynamic tool");

    assert_eq!(
        result.content(),
        &json!({ "tool": "mcp.linear.create_issue" })
    );
}

#[tokio::test]
async fn unknown_tool_returns_controlled_error() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::new();

    let error = registry
        .execute("missing", &workspace, json!({}))
        .await
        .expect_err("unknown tool should fail");

    assert!(matches!(error, HarnessError::ToolNotFound { name } if name == "missing"));
}
