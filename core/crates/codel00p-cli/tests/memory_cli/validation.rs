use super::support::*;

#[test]
fn memory_reject_requires_reason() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-workflow",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
    );

    let output = run_codel00p(
        &db_path,
        &["memory", "reject", "mem-workflow", "--actor", "alice"],
    );

    assert!(!output.status.success());
    assert!(stderr(&output).contains("missing required --reason"));
}

#[test]
fn memory_edit_requires_content() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-workflow",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
    );

    let output = run_codel00p(
        &db_path,
        &["memory", "edit", "mem-workflow", "--actor", "alice"],
    );

    assert!(!output.status.success());
    assert!(stderr(&output).contains("missing required --content"));
}
