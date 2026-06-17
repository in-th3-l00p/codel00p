use super::support::*;

#[test]
fn memory_search_filters_by_max_visibility_scope() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate_with_visibility(
        &db_path,
        "mem-private",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
        MemoryVisibility::Private,
    );
    seed_candidate_with_visibility(
        &db_path,
        "mem-org",
        MemoryKind::Workflow,
        "Run pnpm verify in the org dashboard.",
        "verify",
        MemoryVisibility::Org,
    );
    approve_candidate(&db_path, "mem-private", "alice");
    approve_candidate(&db_path, "mem-org", "alice");

    let default_output = run_codel00p(&db_path, &["memory", "search", "--text", "verify"]);
    let scoped_output = run_codel00p(
        &db_path,
        &[
            "memory",
            "search",
            "--text",
            "verify",
            "--visibility",
            "project",
            "--json",
        ],
    );

    assert!(
        default_output.status.success(),
        "stderr: {}",
        stderr(&default_output)
    );
    assert!(
        scoped_output.status.success(),
        "stderr: {}",
        stderr(&scoped_output)
    );

    // Default (no filter) returns both visibilities.
    let default_ids = stdout(&default_output)
        .lines()
        .map(|line| line.split('\t').next().unwrap_or("").to_string())
        .collect::<Vec<_>>();
    assert_eq!(default_ids, ["mem-org", "mem-private"]);

    // A max-visibility of project excludes the org-scoped memory.
    let records: serde_json::Value =
        serde_json::from_str(&stdout(&scoped_output)).expect("search json");
    let records = records.as_array().expect("record array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "mem-private");
    assert_eq!(records[0]["visibility"], "private");
    assert_eq!(
        records[0]["reason"],
        "matched text verify and visibility project"
    );
}

#[test]
fn memory_show_surfaces_visibility() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate_with_visibility(
        &db_path,
        "mem-team",
        MemoryKind::Workflow,
        "Team-scoped workflow note.",
        "workflow",
        MemoryVisibility::Team,
    );

    let text_output = run_codel00p(&db_path, &["memory", "show", "mem-team"]);
    let json_output = run_codel00p(&db_path, &["memory", "show", "mem-team", "--json"]);

    assert!(
        text_output.status.success(),
        "stderr: {}",
        stderr(&text_output)
    );
    assert!(
        json_output.status.success(),
        "stderr: {}",
        stderr(&json_output)
    );

    assert!(
        stdout(&text_output).contains("visibility: team"),
        "show text: {}",
        stdout(&text_output)
    );
    let record: serde_json::Value = serde_json::from_str(&stdout(&json_output)).expect("show json");
    assert_eq!(record["visibility"], "team");
}
