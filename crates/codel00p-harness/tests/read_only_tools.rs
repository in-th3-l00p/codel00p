use std::fs;

use codel00p_harness::{HarnessError, ToolRegistry, Workspace};
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn read_only_defaults_expose_expected_tools() {
    let registry = ToolRegistry::read_only_defaults();

    assert_eq!(
        registry.names(),
        vec!["list_files", "read_file", "search_text"]
    );
}

#[tokio::test]
async fn list_files_returns_sorted_relative_paths() {
    let dir = tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join("src")).expect("create src");
    fs::write(dir.path().join("README.md"), "readme").expect("write readme");
    fs::write(dir.path().join("src/lib.rs"), "lib").expect("write lib");

    let workspace = Workspace::new(dir.path()).expect("workspace");
    let result = ToolRegistry::read_only_defaults()
        .execute("list_files", &workspace, json!({}))
        .await
        .expect("list files");

    assert_eq!(
        result.content(),
        &json!({
            "files": ["README.md", "src/lib.rs"]
        })
    );
}

#[tokio::test]
async fn read_file_returns_utf8_content() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("README.md"), "Agent Harness\n").expect("write readme");

    let workspace = Workspace::new(dir.path()).expect("workspace");
    let result = ToolRegistry::read_only_defaults()
        .execute("read_file", &workspace, json!({ "path": "README.md" }))
        .await
        .expect("read file");

    assert_eq!(
        result.content(),
        &json!({
            "path": "README.md",
            "content": "Agent Harness\n"
        })
    );
}

#[tokio::test]
async fn read_file_rejects_workspace_escape() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let error = ToolRegistry::read_only_defaults()
        .execute("read_file", &workspace, json!({ "path": "../secret.txt" }))
        .await
        .expect_err("escape rejected");

    assert!(matches!(error, HarnessError::WorkspaceEscape { .. }));
}

#[tokio::test]
async fn search_text_returns_file_line_and_snippet_matches() {
    let dir = tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join("docs")).expect("create docs");
    fs::write(
        dir.path().join("docs/harness.md"),
        "Agent Harness\nProject memory grows here\n",
    )
    .expect("write docs");

    let workspace = Workspace::new(dir.path()).expect("workspace");
    let result = ToolRegistry::read_only_defaults()
        .execute("search_text", &workspace, json!({ "query": "memory" }))
        .await
        .expect("search text");

    assert_eq!(
        result.content(),
        &json!({
            "matches": [{
                "path": "docs/harness.md",
                "line": 2,
                "snippet": "Project memory grows here"
            }]
        })
    );
}
