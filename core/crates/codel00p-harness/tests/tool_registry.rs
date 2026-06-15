use async_trait::async_trait;
use codel00p_harness::{HarnessError, Tool, ToolRegistry, ToolResult, ToolSpec, Workspace};
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

#[test]
fn specs_expose_real_name_description_and_schema() {
    let registry = ToolRegistry::new().with_tool(EchoTool);
    let specs = registry.specs();

    assert_eq!(
        specs,
        vec![ToolSpec::new(
            "echo",
            "Echoes the input payload.",
            json!({
                "type": "object",
                "properties": { "value": { "type": "string" } }
            }),
        )]
    );
}

#[test]
fn default_tool_sets_carry_populated_schemas() {
    let registry = ToolRegistry::read_only_defaults();
    let read = registry
        .specs()
        .into_iter()
        .find(|spec| spec.name == "read_file")
        .expect("read_file registered");

    // Not the old stub: read_file advertises its `path` parameter.
    assert_ne!(read.input_schema, json!({ "type": "object" }));
    assert!(read.input_schema["properties"]["path"].is_object());
    assert!(!read.description.is_empty());
}

#[tokio::test]
async fn execute_validates_arguments_against_schema_before_running() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::read_only_defaults();

    // read_file requires `path`; calling it without one is rejected as a
    // structured InvalidToolInput before any file access.
    let error = registry
        .execute("read_file", &workspace, json!({}))
        .await
        .expect_err("missing required arg should fail validation");

    assert!(matches!(
        error,
        HarnessError::InvalidToolInput { ref name, ref message }
            if name == "read_file" && message.contains("missing required field `path`")
    ));
}
