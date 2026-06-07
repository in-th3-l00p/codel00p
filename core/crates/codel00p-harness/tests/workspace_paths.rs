use std::fs;

use codel00p_harness::{HarnessError, Workspace};
use tempfile::tempdir;

#[test]
fn resolves_relative_paths_inside_workspace_root() {
    let dir = tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join("src")).expect("create src");
    fs::write(dir.path().join("src/lib.rs"), "pub fn demo() {}\n").expect("write file");

    let workspace = Workspace::new(dir.path()).expect("workspace");
    let resolved = workspace.resolve("src/lib.rs").expect("resolve path");

    assert_eq!(
        resolved,
        dir.path().join("src/lib.rs").canonicalize().unwrap()
    );
}

#[test]
fn rejects_parent_directory_traversal() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let error = workspace
        .resolve("../outside.txt")
        .expect_err("escape rejected");

    assert!(matches!(error, HarnessError::WorkspaceEscape { .. }));
}

#[test]
fn rejects_absolute_paths_outside_workspace() {
    let workspace_dir = tempdir().expect("workspace tempdir");
    let outside_dir = tempdir().expect("outside tempdir");
    let outside_file = outside_dir.path().join("secret.txt");
    fs::write(&outside_file, "secret").expect("write outside file");

    let workspace = Workspace::new(workspace_dir.path()).expect("workspace");
    let error = workspace
        .resolve(&outside_file)
        .expect_err("outside path rejected");

    assert!(matches!(error, HarnessError::WorkspaceEscape { .. }));
}

#[test]
fn reads_utf8_files_inside_workspace() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("README.md"), "Agent Harness\n").expect("write readme");

    let workspace = Workspace::new(dir.path()).expect("workspace");
    let content = workspace.read_utf8("README.md").expect("read file");

    assert_eq!(content, "Agent Harness\n");
}
