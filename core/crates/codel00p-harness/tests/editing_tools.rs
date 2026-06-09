use std::fs;

use codel00p_harness::{HarnessError, PermissionScope, ToolRegistry, Workspace};
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn editing_defaults_expose_write_scoped_mutation_tools() {
    let registry = ToolRegistry::editing_defaults();

    assert_eq!(
        registry.names(),
        vec!["apply_patch", "create_file", "delete_file", "update_file"]
    );
    for tool_name in registry.names() {
        assert_eq!(
            registry.permission_scope(&tool_name, &json!({})),
            PermissionScope::WorkspaceWrite
        );
    }
}

#[tokio::test]
async fn create_update_and_delete_file_mutate_workspace_files() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    let created = registry
        .execute(
            "create_file",
            &workspace,
            json!({ "path": "src/lib.rs", "content": "pub fn answer() -> u32 { 41 }\n" }),
        )
        .await
        .expect("create file");
    assert_eq!(
        created.content(),
        &json!({
            "path": "src/lib.rs",
            "operation": "created",
            "bytes_written": 30,
        })
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("src/lib.rs")).expect("read created"),
        "pub fn answer() -> u32 { 41 }\n"
    );

    let updated = registry
        .execute(
            "update_file",
            &workspace,
            json!({ "path": "src/lib.rs", "content": "pub fn answer() -> u32 { 42 }\n" }),
        )
        .await
        .expect("update file");
    assert_eq!(
        updated.content(),
        &json!({
            "path": "src/lib.rs",
            "operation": "updated",
            "bytes_written": 30,
        })
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("src/lib.rs")).expect("read updated"),
        "pub fn answer() -> u32 { 42 }\n"
    );

    let deleted = registry
        .execute("delete_file", &workspace, json!({ "path": "src/lib.rs" }))
        .await
        .expect("delete file");
    assert_eq!(
        deleted.content(),
        &json!({ "path": "src/lib.rs", "operation": "deleted" })
    );
    assert!(!dir.path().join("src/lib.rs").exists());
}

#[tokio::test]
async fn create_and_update_enforce_expected_file_state() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("README.md"), "existing\n").expect("write file");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    let create_error = registry
        .execute(
            "create_file",
            &workspace,
            json!({ "path": "README.md", "content": "new\n" }),
        )
        .await
        .expect_err("create should reject existing file");
    assert!(
        matches!(create_error, HarnessError::ToolFailed { name, message } if name == "create_file" && message.contains("already exists"))
    );

    let update_error = registry
        .execute(
            "update_file",
            &workspace,
            json!({ "path": "missing.md", "content": "new\n" }),
        )
        .await
        .expect_err("update should reject missing file");
    assert!(
        matches!(update_error, HarnessError::ToolFailed { name, message } if name == "update_file" && message.contains("does not exist"))
    );
}

#[tokio::test]
async fn editing_tools_reject_workspace_escape_paths() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    let error = registry
        .execute(
            "create_file",
            &workspace,
            json!({ "path": "../escape.txt", "content": "nope\n" }),
        )
        .await
        .expect_err("workspace escape should fail");

    assert!(matches!(error, HarnessError::WorkspaceEscape { .. }));
}

#[tokio::test]
async fn apply_patch_performs_exact_replacements_and_reports_changes() {
    let dir = tempdir().expect("tempdir");
    fs::create_dir(dir.path().join("src")).expect("create src");
    fs::write(
        dir.path().join("src/lib.rs"),
        "pub fn answer() -> u32 { 41 }\n",
    )
    .expect("write file");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    let result = registry
        .execute(
            "apply_patch",
            &workspace,
            json!({
                "changes": [
                    {
                        "path": "src/lib.rs",
                        "find": "41",
                        "replace": "42"
                    }
                ]
            }),
        )
        .await
        .expect("apply patch");

    assert_eq!(
        result.content(),
        &json!({
            "operation": "patched",
            "changes": [
                {
                    "path": "src/lib.rs",
                    "replacements": 1,
                    "bytes_written": 30
                }
            ]
        })
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("src/lib.rs")).expect("read patched"),
        "pub fn answer() -> u32 { 42 }\n"
    );
}

#[tokio::test]
async fn apply_patch_rejects_missing_match_without_writing() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("README.md"), "original\n").expect("write file");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    let error = registry
        .execute(
            "apply_patch",
            &workspace,
            json!({
                "changes": [
                    {
                        "path": "README.md",
                        "find": "absent",
                        "replace": "changed"
                    }
                ]
            }),
        )
        .await
        .expect_err("missing match should fail");

    assert!(
        matches!(error, HarnessError::ToolFailed { name, message } if name == "apply_patch" && message.contains("find text was not present"))
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("README.md")).expect("read file"),
        "original\n"
    );
}
