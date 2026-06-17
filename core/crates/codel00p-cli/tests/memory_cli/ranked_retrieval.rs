use super::support::*;

#[test]
fn memory_retrieve_ranks_approved_memory_by_similarity() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-close",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing to the main branch.",
        "verify",
    );
    seed_candidate(
        &db_path,
        "mem-partial",
        MemoryKind::Workflow,
        "Run pnpm verify after editing the provider policy.",
        "verify",
    );
    seed_candidate(
        &db_path,
        "mem-unrelated",
        MemoryKind::Workflow,
        "Configure the colorful unicorn dashboard widget.",
        "ui",
    );
    approve_candidate(&db_path, "mem-close", "alice");
    approve_candidate(&db_path, "mem-partial", "alice");
    approve_candidate(&db_path, "mem-unrelated", "alice");

    let output = run_codel00p(
        &db_path,
        &[
            "memory",
            "retrieve",
            "Run pnpm verify before pushing main branch.",
        ],
    );
    let output_json = run_codel00p(
        &db_path,
        &[
            "memory",
            "retrieve",
            "Run pnpm verify before pushing main branch.",
            "--json",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        output_json.status.success(),
        "stderr: {}",
        stderr(&output_json)
    );

    // The more-similar memory ranks before the partial match; the unrelated
    // memory is dropped (no shared tokens).
    let lines = stdout(&output)
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with("mem-close\tapproved\tworkflow\t"));
    assert!(lines[1].starts_with("mem-partial\tapproved\tworkflow\t"));

    let records: serde_json::Value =
        serde_json::from_str(&stdout(&output_json)).expect("retrieve json");
    let records = records.as_array().expect("record array");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["id"], "mem-close");
    assert_eq!(records[1]["id"], "mem-partial");
    // Scores are present and ordered descending.
    let first = records[0]["score"].as_u64().expect("first score");
    let second = records[1]["score"].as_u64().expect("second score");
    assert!(first > second, "expected {first} > {second}");
    assert_eq!(records[0]["status"], "approved");
    assert_eq!(records[0]["kind"], "workflow");
    assert_eq!(records[0]["tags"], serde_json::json!(["verify"]));
}

#[test]
fn memory_retrieve_filters_by_kind_before_ranking() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-workflow",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main branch.",
        "verify",
    );
    seed_candidate(
        &db_path,
        "mem-architecture",
        MemoryKind::Architecture,
        "Run pnpm verify before pushing main branch.",
        "verify",
    );
    approve_candidate(&db_path, "mem-workflow", "alice");
    approve_candidate(&db_path, "mem-architecture", "alice");

    let output_json = run_codel00p(
        &db_path,
        &[
            "memory",
            "retrieve",
            "Run pnpm verify before pushing main branch.",
            "--kind",
            "workflow",
            "--json",
        ],
    );

    assert!(
        output_json.status.success(),
        "stderr: {}",
        stderr(&output_json)
    );
    let records: serde_json::Value =
        serde_json::from_str(&stdout(&output_json)).expect("retrieve json");
    let records = records.as_array().expect("record array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "mem-workflow");
}

#[test]
fn memory_retrieve_excludes_sensitive_unless_requested() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-normal",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main branch.",
        "verify",
    );
    seed_candidate_with_sensitivity(
        &db_path,
        "mem-sensitive",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main branch with the private credential.",
        "verify",
        MemorySensitivity::Sensitive,
    );
    approve_candidate(&db_path, "mem-normal", "alice");
    approve_candidate(&db_path, "mem-sensitive", "alice");

    let default_json = run_codel00p(
        &db_path,
        &[
            "memory",
            "retrieve",
            "Run pnpm verify before pushing main branch.",
            "--json",
        ],
    );
    let sensitive_json = run_codel00p(
        &db_path,
        &[
            "memory",
            "retrieve",
            "Run pnpm verify before pushing main branch.",
            "--sensitive",
            "--json",
        ],
    );

    assert!(default_json.status.success());
    assert!(sensitive_json.status.success());

    let default_records: serde_json::Value =
        serde_json::from_str(&stdout(&default_json)).expect("default json");
    let default_records = default_records.as_array().expect("array");
    assert_eq!(default_records.len(), 1);
    assert_eq!(default_records[0]["id"], "mem-normal");

    let sensitive_records: serde_json::Value =
        serde_json::from_str(&stdout(&sensitive_json)).expect("sensitive json");
    let sensitive_records = sensitive_records.as_array().expect("array");
    assert_eq!(sensitive_records.len(), 1);
    assert_eq!(sensitive_records[0]["id"], "mem-sensitive");
}

#[test]
fn memory_retrieve_requires_a_query_string() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");

    let output = run_codel00p(&db_path, &["memory", "retrieve", "--json"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("query"),
        "stderr: {}",
        stderr(&output)
    );
}
