use std::fs;

use codel00p_harness::{HarnessError, PermissionScope, ToolRegistry, Workspace};
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn editing_defaults_expose_write_scoped_mutation_tools() {
    let registry = ToolRegistry::editing_defaults();

    assert_eq!(
        registry.names(),
        vec![
            "apply_patch",
            "copy_file",
            "create_file",
            "delete_file",
            "move_file",
            "update_file"
        ]
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
                    "bytes_written": 30,
                    "strategy": "exact"
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

#[tokio::test]
async fn apply_patch_chains_multiple_changes_to_the_same_file() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("config.toml"), "foo bar baz\n").expect("write file");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    let result = registry
        .execute(
            "apply_patch",
            &workspace,
            json!({
                "changes": [
                    { "path": "config.toml", "find": "foo", "replace": "FOO" },
                    { "path": "config.toml", "find": "bar", "replace": "BAR" }
                ]
            }),
        )
        .await
        .expect("apply patch");

    // Both edits land — the second change must see the result of the first
    // rather than re-reading the original file (which would lose "FOO").
    assert_eq!(
        fs::read_to_string(dir.path().join("config.toml")).expect("read patched"),
        "FOO BAR baz\n"
    );
    assert_eq!(
        result.content()["changes"]
            .as_array()
            .expect("changes")
            .len(),
        2
    );
}

#[tokio::test]
async fn apply_patch_lets_a_later_change_match_an_earlier_replacement() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("notes.txt"), "alpha\n").expect("write file");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    // The second change's `find` only exists *after* the first change applies,
    // proving the batch composes in order against the accumulating content.
    registry
        .execute(
            "apply_patch",
            &workspace,
            json!({
                "changes": [
                    { "path": "notes.txt", "find": "alpha", "replace": "beta" },
                    { "path": "notes.txt", "find": "beta", "replace": "gamma" }
                ]
            }),
        )
        .await
        .expect("apply patch");

    assert_eq!(
        fs::read_to_string(dir.path().join("notes.txt")).expect("read patched"),
        "gamma\n"
    );
}

#[tokio::test]
async fn apply_patch_batch_is_atomic_when_a_later_change_fails() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("data.txt"), "keep me\n").expect("write file");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    let error = registry
        .execute(
            "apply_patch",
            &workspace,
            json!({
                "changes": [
                    { "path": "data.txt", "find": "keep me", "replace": "changed" },
                    { "path": "data.txt", "find": "absent", "replace": "noop" }
                ]
            }),
        )
        .await
        .expect_err("batch with a missing match should fail");

    assert!(matches!(error, HarnessError::ToolFailed { name, .. } if name == "apply_patch"));
    // The first change succeeded in memory but the batch failed before any write,
    // so the file on disk is untouched.
    assert_eq!(
        fs::read_to_string(dir.path().join("data.txt")).expect("read file"),
        "keep me\n"
    );
}

async fn patch_once(
    workspace: &Workspace,
    registry: &ToolRegistry,
    path: &str,
    find: &str,
    replace: &str,
) -> Result<codel00p_harness::tool_result::ToolResult, HarnessError> {
    registry
        .execute(
            "apply_patch",
            workspace,
            json!({
                "changes": [
                    { "path": path, "find": find, "replace": replace }
                ]
            }),
        )
        .await
}

#[tokio::test]
async fn apply_patch_tolerates_trailing_whitespace_drift() {
    let dir = tempdir().expect("tempdir");
    // The first matched line carries trailing spaces the model omits. Because the
    // find spans two lines, the exact substring fast path cannot match.
    fs::write(dir.path().join("a.txt"), "let x = 1;   \nlet y = 2;\n").expect("write");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    let result = patch_once(
        &workspace,
        &registry,
        "a.txt",
        "let x = 1;\nlet y = 2;\n",
        "let x = 9;\nlet y = 8;\n",
    )
    .await
    .expect("trailing whitespace tolerant patch");

    assert_eq!(
        result.content()["changes"][0]["strategy"],
        json!("trailing-whitespace")
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).expect("read"),
        "let x = 9;\nlet y = 8;\n"
    );
}

#[tokio::test]
async fn apply_patch_tolerates_indentation_drift_and_preserves_file_indent() {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("svc.rs"),
        "fn run() {\n        let total = a + b;\n        emit(total);\n}\n",
    )
    .expect("write");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    // Model sends 4-space indentation; file uses 8 spaces.
    let result = patch_once(
        &workspace,
        &registry,
        "svc.rs",
        "    let total = a + b;\n    emit(total);\n",
        "    let total = a + b + c;\n    emit(total);\n",
    )
    .await
    .expect("indentation tolerant patch");

    assert_eq!(
        result.content()["changes"][0]["strategy"],
        json!("indentation")
    );
    // The file's actual 8-space indentation is preserved.
    assert_eq!(
        fs::read_to_string(dir.path().join("svc.rs")).expect("read"),
        "fn run() {\n        let total = a + b + c;\n        emit(total);\n}\n",
    );
}

#[tokio::test]
async fn apply_patch_tolerates_crlf_line_endings() {
    let dir = tempdir().expect("tempdir");
    fs::write(
        dir.path().join("win.txt"),
        "first line\r\nsecond line\r\nthird line\r\n",
    )
    .expect("write");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    // Model provides LF; file is CRLF.
    let result = patch_once(
        &workspace,
        &registry,
        "win.txt",
        "second line\n",
        "SECOND LINE\n",
    )
    .await
    .expect("crlf tolerant patch");

    assert_eq!(
        result.content()["changes"][0]["strategy"],
        json!("line-ending")
    );
    // Bytes outside the matched region (including CRLF on other lines) are intact.
    assert_eq!(
        fs::read_to_string(dir.path().join("win.txt")).expect("read"),
        "first line\r\nSECOND LINE\nthird line\r\n",
    );
}

#[tokio::test]
async fn apply_patch_rejects_multiple_matches_without_replace_all() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("dup.txt"), "todo\nkeep\ntodo\n").expect("write");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    let error = patch_once(&workspace, &registry, "dup.txt", "todo", "done")
        .await
        .expect_err("ambiguous match should fail");

    let HarnessError::ToolFailed { name, message } = error else {
        panic!("expected ToolFailed, got {error:?}");
    };
    assert_eq!(name, "apply_patch");
    assert!(message.contains("2 matches"), "message was: {message}");
    assert!(message.contains("replace_all"), "message was: {message}");
    // Nothing was written.
    assert_eq!(
        fs::read_to_string(dir.path().join("dup.txt")).expect("read"),
        "todo\nkeep\ntodo\n"
    );
}

#[tokio::test]
async fn apply_patch_replace_all_replaces_every_occurrence() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("dup.txt"), "todo\nkeep\ntodo\n").expect("write");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    let result = registry
        .execute(
            "apply_patch",
            &workspace,
            json!({
                "changes": [
                    { "path": "dup.txt", "find": "todo", "replace": "done", "replace_all": true }
                ]
            }),
        )
        .await
        .expect("replace_all patch");

    assert_eq!(result.content()["changes"][0]["replacements"], json!(2));
    assert_eq!(
        fs::read_to_string(dir.path().join("dup.txt")).expect("read"),
        "done\nkeep\ndone\n"
    );
}

#[tokio::test]
async fn apply_patch_not_found_error_hints_at_whitespace_difference() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("code.rs"), "    let value = compute();\n").expect("write");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    // Internal whitespace differs (two spaces around `=`), which the line-based
    // tolerant strategies do not collapse, so this is a genuine miss.
    let error = patch_once(
        &workspace,
        &registry,
        "code.rs",
        "let  value  =  compute();",
        "let value = recompute();",
    )
    .await
    .expect_err("internal whitespace mismatch should fail");

    let HarnessError::ToolFailed { message, .. } = error else {
        panic!("expected ToolFailed, got {error:?}");
    };
    assert!(
        message.contains("whitespace"),
        "expected a whitespace hint, got: {message}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("code.rs")).expect("read"),
        "    let value = compute();\n"
    );
}

#[tokio::test]
async fn apply_patch_real_multi_line_code_edit_preserves_surrounding_bytes() {
    let dir = tempdir().expect("tempdir");
    fs::create_dir(dir.path().join("src")).expect("mkdir");
    let original = "use std::io;\n\nfn parse(input: &str) -> u32 {\n    let trimmed = input.trim();\n    trimmed.parse().unwrap_or(0)\n}\n\nfn main() {\n    println!(\"{}\", parse(\"7\"));\n}\n";
    fs::write(dir.path().join("src/main.rs"), original).expect("write");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    let result = patch_once(
        &workspace,
        &registry,
        "src/main.rs",
        "    let trimmed = input.trim();\n    trimmed.parse().unwrap_or(0)\n",
        "    let trimmed = input.trim();\n    trimmed.parse().unwrap_or(u32::MAX)\n",
    )
    .await
    .expect("multi-line edit");

    assert_eq!(result.content()["changes"][0]["strategy"], json!("exact"));
    assert_eq!(
        fs::read_to_string(dir.path().join("src/main.rs")).expect("read"),
        "use std::io;\n\nfn parse(input: &str) -> u32 {\n    let trimmed = input.trim();\n    trimmed.parse().unwrap_or(u32::MAX)\n}\n\nfn main() {\n    println!(\"{}\", parse(\"7\"));\n}\n",
    );
}

#[tokio::test]
async fn move_file_relocates_content_and_removes_source() {
    let dir = tempdir().expect("tempdir");
    // Binary / non-UTF-8 payload so we exercise byte-safety, not just text.
    let payload: &[u8] = &[0x00, 0xff, 0xfe, b'h', b'i', 0x80, 0x0a];
    fs::write(dir.path().join("source.bin"), payload).expect("write source");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    let moved = registry
        .execute(
            "move_file",
            &workspace,
            json!({ "from": "source.bin", "to": "nested/dest.bin" }),
        )
        .await
        .expect("move file");

    assert_eq!(
        moved.content(),
        &json!({
            "from": "source.bin",
            "to": "nested/dest.bin",
            "operation": "moved",
            "bytes_written": payload.len(),
        })
    );
    assert!(!dir.path().join("source.bin").exists());
    assert_eq!(
        fs::read(dir.path().join("nested/dest.bin")).expect("read dest"),
        payload
    );
}

#[tokio::test]
async fn copy_file_duplicates_content_and_keeps_source() {
    let dir = tempdir().expect("tempdir");
    let payload: &[u8] = &[0xde, 0xad, 0xbe, 0xef, 0x00, 0x0a];
    fs::write(dir.path().join("source.bin"), payload).expect("write source");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    let copied = registry
        .execute(
            "copy_file",
            &workspace,
            json!({ "from": "source.bin", "to": "copy.bin" }),
        )
        .await
        .expect("copy file");

    assert_eq!(
        copied.content(),
        &json!({
            "from": "source.bin",
            "to": "copy.bin",
            "operation": "copied",
            "bytes_written": payload.len(),
        })
    );
    assert_eq!(
        fs::read(dir.path().join("source.bin")).expect("read source"),
        payload,
        "copy must leave the source intact"
    );
    assert_eq!(
        fs::read(dir.path().join("copy.bin")).expect("read copy"),
        payload
    );
}

#[tokio::test]
async fn move_and_copy_reject_missing_source_and_existing_dest() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("a.txt"), "a\n").expect("write a");
    fs::write(dir.path().join("b.txt"), "b\n").expect("write b");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    let missing = registry
        .execute(
            "move_file",
            &workspace,
            json!({ "from": "nope.txt", "to": "x.txt" }),
        )
        .await
        .expect_err("missing source should fail");
    assert!(
        matches!(missing, HarnessError::ToolFailed { name, message } if name == "move_file" && message.contains("does not exist"))
    );

    let occupied = registry
        .execute(
            "copy_file",
            &workspace,
            json!({ "from": "a.txt", "to": "b.txt" }),
        )
        .await
        .expect_err("existing dest should fail");
    assert!(
        matches!(occupied, HarnessError::ToolFailed { name, message } if name == "copy_file" && message.contains("already exists"))
    );
    // b.txt must be untouched.
    assert_eq!(
        fs::read_to_string(dir.path().join("b.txt")).expect("read b"),
        "b\n"
    );
}

#[tokio::test]
async fn move_file_overwrites_dest_when_requested() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("a.txt"), "new\n").expect("write a");
    fs::write(dir.path().join("b.txt"), "old\n").expect("write b");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    registry
        .execute(
            "move_file",
            &workspace,
            json!({ "from": "a.txt", "to": "b.txt", "overwrite": true }),
        )
        .await
        .expect("move with overwrite");

    assert!(!dir.path().join("a.txt").exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("b.txt")).expect("read b"),
        "new\n"
    );
}

#[tokio::test]
async fn move_and_copy_reject_workspace_escape_paths() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("a.txt"), "a\n").expect("write a");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    let escape_to = registry
        .execute(
            "copy_file",
            &workspace,
            json!({ "from": "a.txt", "to": "../escape.txt" }),
        )
        .await
        .expect_err("escaping dest should fail");
    assert!(matches!(escape_to, HarnessError::WorkspaceEscape { .. }));

    let escape_from = registry
        .execute(
            "move_file",
            &workspace,
            json!({ "from": "../../etc/passwd", "to": "a-copy.txt" }),
        )
        .await
        .expect_err("escaping source should fail");
    assert!(matches!(escape_from, HarnessError::WorkspaceEscape { .. }));
}

#[tokio::test]
async fn update_file_description_nudges_toward_apply_patch() {
    use codel00p_harness::{Tool, UpdateFileTool};
    let description = UpdateFileTool.description();
    assert!(
        description.contains("apply_patch"),
        "update_file description should nudge toward apply_patch: {description}"
    );
}

// --- apply_patch input-shape robustness (smaller-model tolerance) ---
//
// These cover the deviations from the canonical
// `{ "changes": [{ "path", "find", "replace" }] }` shape that smaller models
// produce, each of which previously dead-ended on "missing string field `path`"
// or "missing array field `changes`". The fix normalizes them into the same
// atomic batch the canonical shape uses.

#[tokio::test]
async fn apply_patch_accepts_a_flat_single_change_without_the_changes_wrapper() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("index.html"), "<h1>old</h1>\n").expect("write file");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    let result = registry
        .execute(
            "apply_patch",
            &workspace,
            json!({ "path": "index.html", "find": "old", "replace": "new" }),
        )
        .await
        .expect("flat change should apply");

    assert_eq!(result.content()["changes"][0]["path"], json!("index.html"));
    assert_eq!(
        fs::read_to_string(dir.path().join("index.html")).expect("read"),
        "<h1>new</h1>\n"
    );
}

#[tokio::test]
async fn apply_patch_shares_a_top_level_path_with_changes_that_omit_it() {
    // The "name the file once, then list edits" shape: `path` at the top level,
    // each change carrying only find/replace.
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("app.js"), "let a = 1;\nlet b = 2;\n").expect("write file");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    registry
        .execute(
            "apply_patch",
            &workspace,
            json!({
                "path": "app.js",
                "changes": [
                    { "find": "let a = 1;", "replace": "const a = 1;" },
                    { "find": "let b = 2;", "replace": "const b = 2;" }
                ]
            }),
        )
        .await
        .expect("top-level path should fill in for changes");

    assert_eq!(
        fs::read_to_string(dir.path().join("app.js")).expect("read"),
        "const a = 1;\nconst b = 2;\n"
    );
}

#[tokio::test]
async fn apply_patch_accepts_old_string_new_string_field_aliases() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("main.rs"), "fn main() { todo!() }\n").expect("write file");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    let result = registry
        .execute(
            "apply_patch",
            &workspace,
            json!({
                "changes": [
                    { "file": "main.rs", "old_string": "todo!()", "new_string": "()" }
                ]
            }),
        )
        .await
        .expect("aliased fields should apply");

    assert_eq!(result.content()["changes"][0]["path"], json!("main.rs"));
    assert_eq!(
        fs::read_to_string(dir.path().join("main.rs")).expect("read"),
        "fn main() { () }\n"
    );
}

#[tokio::test]
async fn apply_patch_accepts_changes_given_as_a_single_object() {
    // Some models emit `changes` as one object instead of a one-element array.
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("notes.txt"), "draft\n").expect("write file");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    registry
        .execute(
            "apply_patch",
            &workspace,
            json!({ "changes": { "path": "notes.txt", "find": "draft", "replace": "final" } }),
        )
        .await
        .expect("single-object changes should apply");

    assert_eq!(
        fs::read_to_string(dir.path().join("notes.txt")).expect("read"),
        "final\n"
    );
}

#[tokio::test]
async fn apply_patch_still_reports_missing_path_when_none_can_be_resolved() {
    // With neither a per-change nor a top-level path, the actionable error is
    // preserved (no silent success).
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("a.txt"), "x\n").expect("write file");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::editing_defaults();

    let error = registry
        .execute(
            "apply_patch",
            &workspace,
            json!({ "changes": [{ "find": "x", "replace": "y" }] }),
        )
        .await
        .expect_err("missing path should still fail");

    assert!(matches!(
        error,
        HarnessError::InvalidToolInput { name, message }
            if name == "apply_patch" && message.contains("missing string field `path`")
    ));
}
