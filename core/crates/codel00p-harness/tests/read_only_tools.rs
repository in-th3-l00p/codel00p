use std::fs;

use codel00p_harness::{HarnessError, ToolRegistry, Workspace};
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn read_only_defaults_expose_expected_tools() {
    let registry = ToolRegistry::read_only_defaults();

    assert_eq!(
        registry.names(),
        vec![
            "find_files",
            "grep",
            "list_files",
            "read_file",
            "search_text"
        ]
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

#[tokio::test]
async fn read_file_returns_a_line_window_with_metadata() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("big.txt"), "l1\nl2\nl3\nl4\nl5\n").expect("write");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::read_only_defaults();

    let result = registry
        .execute(
            "read_file",
            &workspace,
            json!({ "path": "big.txt", "offset": 2, "limit": 2 }),
        )
        .await
        .expect("read window");

    assert_eq!(
        result.content(),
        &json!({
            "path": "big.txt",
            "content": "l2\nl3",
            "start_line": 2,
            "line_count": 2,
            "total_lines": 5,
            "has_more": true,
        })
    );
}

#[tokio::test]
async fn read_file_window_to_end_reports_no_more() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("f.txt"), "a\nb\nc\n").expect("write");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::read_only_defaults();

    let result = registry
        .execute(
            "read_file",
            &workspace,
            json!({ "path": "f.txt", "offset": 3 }),
        )
        .await
        .expect("read window");

    assert_eq!(result.content()["content"], json!("c"));
    assert_eq!(result.content()["has_more"], json!(false));
    assert_eq!(result.content()["total_lines"], json!(3));
}

#[tokio::test]
async fn search_text_paginates_matches() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("a.txt"), "hit 1\nno\nhit 2\nhit 3\nhit 4\n").expect("write");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::read_only_defaults();

    let result = registry
        .execute(
            "search_text",
            &workspace,
            json!({ "query": "hit", "offset": 1, "limit": 2 }),
        )
        .await
        .expect("search page");

    let matches = result.content()["matches"].as_array().expect("matches");
    assert_eq!(matches.len(), 2);
    // Skipped the first match (line 1); window starts at line 3.
    assert_eq!(matches[0]["line"], json!(3));
    assert_eq!(matches[1]["line"], json!(4));
    assert_eq!(result.content()["offset"], json!(1));
    assert_eq!(result.content()["has_more"], json!(true));
}
