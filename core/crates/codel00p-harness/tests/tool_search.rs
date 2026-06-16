//! Tests for progressive disclosure: deferred tools stay executable but are
//! advertised only through the synthetic `tool_search` / `tool_describe` tools.

use async_trait::async_trait;
use codel00p_harness::{
    HarnessError, PermissionScope, TOOL_DESCRIBE, TOOL_SEARCH, Tool, ToolRegistry, ToolResult,
    Workspace,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tempfile::tempdir;

/// A trivial tool with a configurable name/description used to populate a large
/// hidden tool set.
struct FakeTool {
    name: String,
    description: String,
}

#[async_trait]
impl Tool for FakeTool {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn input_schema(&self) -> Value {
        json!({ "type": "object", "properties": { "value": { "type": "string" } } })
    }
    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::ReadOnly
    }
    async fn execute(
        &self,
        _workspace: &Workspace,
        _input: Value,
    ) -> Result<ToolResult, HarnessError> {
        Ok(ToolResult::json(json!({ "tool": self.name })))
    }
}

fn fake(name: &str, description: &str) -> Arc<dyn Tool> {
    Arc::new(FakeTool {
        name: name.to_string(),
        description: description.to_string(),
    })
}

fn workspace() -> (tempfile::TempDir, Workspace) {
    let dir = tempdir().unwrap();
    let workspace = Workspace::new(dir.path()).unwrap();
    (dir, workspace)
}

fn names(specs: &[codel00p_harness::ToolSpec]) -> Vec<String> {
    specs.iter().map(|s| s.name.clone()).collect()
}

#[test]
fn no_deferred_tools_advertises_everything() {
    let registry = ToolRegistry::read_only_defaults();
    // Without deferral, advertised == full catalog and no meta tools appear.
    assert_eq!(registry.advertised_specs().len(), registry.specs().len());
    assert!(!names(&registry.advertised_specs()).contains(&TOOL_SEARCH.to_string()));
}

#[test]
fn deferred_tools_are_hidden_and_meta_tools_appear() {
    let registry = ToolRegistry::read_only_defaults()
        .with_deferred_tool_arc(fake("github_create_issue", "Open a GitHub issue"))
        .with_deferred_tool_arc(fake("github_list_prs", "List GitHub pull requests"));

    let advertised = names(&registry.advertised_specs());
    // Core read-only tools are still advertised.
    assert!(advertised.contains(&"read_file".to_string()));
    // Deferred tools are NOT advertised directly.
    assert!(!advertised.contains(&"github_create_issue".to_string()));
    // The two disclosure tools are advertised instead.
    assert!(advertised.contains(&TOOL_SEARCH.to_string()));
    assert!(advertised.contains(&TOOL_DESCRIBE.to_string()));
    // But the full catalog still knows about the deferred tools.
    assert!(
        registry
            .names()
            .contains(&"github_create_issue".to_string())
    );
    assert_eq!(registry.deferred_names().len(), 2);
}

#[tokio::test]
async fn tool_search_ranks_by_query() {
    let (_dir, ws) = workspace();
    let registry = ToolRegistry::read_only_defaults()
        .with_deferred_tool_arc(fake("github_create_issue", "Open a GitHub issue"))
        .with_deferred_tool_arc(fake("github_list_prs", "List GitHub pull requests"))
        .with_deferred_tool_arc(fake("slack_post_message", "Send a Slack message"));

    let result = registry
        .execute(TOOL_SEARCH, &ws, json!({ "query": "github issue" }))
        .await
        .unwrap();
    let content = result.content();
    let tools = content["tools"].as_array().unwrap();

    // Both github tools match; the issue tool ranks first (name + description hit).
    assert_eq!(tools[0]["name"], "github_create_issue");
    let returned: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(returned.contains(&"github_list_prs"));
    assert!(!returned.contains(&"slack_post_message"));
    assert_eq!(content["deferred_total"], 3);
}

#[tokio::test]
async fn tool_search_without_query_lists_all_hidden() {
    let (_dir, ws) = workspace();
    let registry = ToolRegistry::new()
        .with_deferred_tool_arc(fake("a_tool", "alpha"))
        .with_deferred_tool_arc(fake("b_tool", "beta"));

    let result = registry.execute(TOOL_SEARCH, &ws, json!({})).await.unwrap();
    let tools = result.content()["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 2);
}

#[tokio::test]
async fn tool_describe_returns_schema_for_hidden_tool() {
    let (_dir, ws) = workspace();
    let registry = ToolRegistry::read_only_defaults()
        .with_deferred_tool_arc(fake("github_create_issue", "Open a GitHub issue"));

    let result = registry
        .execute(
            TOOL_DESCRIBE,
            &ws,
            json!({ "names": ["github_create_issue", "nope"] }),
        )
        .await
        .unwrap();
    let tools = result.content()["tools"].as_array().unwrap();
    assert_eq!(tools[0]["name"], "github_create_issue");
    assert!(tools[0]["input_schema"].is_object());
    assert_eq!(tools[1]["error"], "unknown tool");
}

#[tokio::test]
async fn deferred_tool_executes_normally_once_known() {
    let (_dir, ws) = workspace();
    let registry =
        ToolRegistry::new().with_deferred_tool_arc(fake("github_create_issue", "Open an issue"));

    // A deferred tool is still callable by name — discovery does not gate execution.
    let result = registry
        .execute("github_create_issue", &ws, json!({ "value": "x" }))
        .await
        .unwrap();
    assert_eq!(result.content()["tool"], "github_create_issue");
}

#[test]
fn progressive_disclosure_threshold_keeps_small_sets_visible() {
    // 5 read-only tools, threshold 10 → nothing deferred.
    let registry = ToolRegistry::read_only_defaults().with_progressive_disclosure(10, &[]);
    assert!(registry.deferred_names().is_empty());
    assert_eq!(registry.advertised_specs().len(), 5);
}

#[test]
fn progressive_disclosure_threshold_hides_excess_tools() {
    let mut registry = ToolRegistry::read_only_defaults();
    for i in 0..20 {
        registry = registry.with_tool_arc(fake(&format!("mcp_tool_{i}"), "an mcp tool"));
    }
    let registry = registry.with_progressive_disclosure(10, &["read_file", "grep", "find_files"]);

    let advertised = names(&registry.advertised_specs());
    assert!(advertised.contains(&"read_file".to_string()));
    assert!(advertised.contains(&"grep".to_string()));
    assert!(advertised.contains(&TOOL_SEARCH.to_string()));
    // The mcp tools and non-kept core tools are hidden.
    assert!(!advertised.contains(&"mcp_tool_0".to_string()));
    assert!(!advertised.contains(&"list_files".to_string()));
}

#[test]
fn re_adding_a_deferred_tool_advertised_un_defers_it() {
    let registry = ToolRegistry::new()
        .with_deferred_tool_arc(fake("x", "hidden"))
        .with_tool_arc(fake("x", "now visible"));
    assert!(registry.deferred_names().is_empty());
    assert!(names(&registry.advertised_specs()).contains(&"x".to_string()));
}

#[tokio::test]
async fn tool_describe_rejects_missing_names() {
    let (_dir, ws) = workspace();
    let registry = ToolRegistry::new().with_deferred_tool_arc(fake("x", "hidden"));
    let error = registry
        .execute(TOOL_DESCRIBE, &ws, json!({}))
        .await
        .unwrap_err();
    assert!(matches!(error, HarnessError::InvalidToolInput { .. }));
}
