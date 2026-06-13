use super::support::*;

#[test]
fn memory_list_prints_filtered_candidates() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-workflow",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main after editing provider policy.",
        "verify",
    );
    seed_candidate(
        &db_path,
        "mem-architecture",
        MemoryKind::Architecture,
        "The harness owns tool execution.",
        "harness",
    );

    let output = run_codel00p(
        &db_path,
        &[
            "memory",
            "list",
            "--status",
            "candidate",
            "--kind",
            "workflow",
            "--tag",
            "verify",
        ],
    );
    let output_json = run_codel00p(
        &db_path,
        &[
            "memory",
            "list",
            "--status",
            "candidate",
            "--kind",
            "workflow",
            "--tag",
            "verify",
            "--json",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        output_json.status.success(),
        "stderr: {}",
        stderr(&output_json)
    );
    assert_eq!(
        stdout(&output),
        "mem-workflow\tcandidate\tworkflow\tRun pnpm verify before pushing main after editing provider policy.\n"
    );
    let records: serde_json::Value =
        serde_json::from_str(&stdout(&output_json)).expect("list json");
    let records = records.as_array().expect("record array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "mem-workflow");
    assert_eq!(records[0]["status"], "candidate");
    assert_eq!(records[0]["kind"], "workflow");
    assert_eq!(
        records[0]["content"],
        "Run pnpm verify before pushing main after editing provider policy."
    );
    assert_eq!(records[0]["quality"]["score"], 100);
    assert_eq!(records[0]["quality"]["findings"], serde_json::json!([]));
    assert_eq!(records[0]["tags"], serde_json::json!(["verify"]));
    assert_eq!(records[0]["source"]["session_id"], "session-cli");
    assert_eq!(records[0]["source"]["turn_id"], "turn-cli");
    assert_eq!(records[0]["source_uri"], "codel00p://sessions/session-cli");
}

#[test]
fn memory_list_json_prefers_explicit_source_uri() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate_with_source(
        &db_path,
        "mem-workflow-source",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
        source().with_uri("https://github.com/in-th3-l00p/codel00p/pull/1"),
    );

    let output = run_codel00p(&db_path, &["memory", "list", "--json"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let records: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("list json");
    let records = records.as_array().expect("record array");
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0]["source"]["uri"],
        "https://github.com/in-th3-l00p/codel00p/pull/1"
    );
    assert_eq!(
        records[0]["source_uri"],
        "https://github.com/in-th3-l00p/codel00p/pull/1"
    );
}

#[test]
fn memory_search_retrieves_approved_memory() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-workflow",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
    );
    seed_candidate(
        &db_path,
        "mem-candidate",
        MemoryKind::Workflow,
        "Candidate verify reminder.",
        "verify",
    );
    approve_candidate(&db_path, "mem-workflow", "alice");

    let output = run_codel00p(
        &db_path,
        &[
            "memory", "search", "--text", "verify", "--kind", "workflow", "--tag", "verify",
        ],
    );
    let output_json = run_codel00p(
        &db_path,
        &[
            "memory", "search", "--text", "verify", "--kind", "workflow", "--tag", "verify",
            "--json",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        output_json.status.success(),
        "stderr: {}",
        stderr(&output_json)
    );
    assert_eq!(
        stdout(&output),
        "mem-workflow\tapproved\tworkflow\tmatched kind workflow and tag verify and text verify\tRun pnpm verify before pushing main.\n"
    );
    let records: serde_json::Value =
        serde_json::from_str(&stdout(&output_json)).expect("search json");
    let records = records.as_array().expect("record array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "mem-workflow");
    assert_eq!(records[0]["status"], "approved");
    assert_eq!(records[0]["kind"], "workflow");
    assert_eq!(
        records[0]["content"],
        "Run pnpm verify before pushing main."
    );
    assert_eq!(
        records[0]["reason"],
        "matched kind workflow and tag verify and text verify"
    );
    assert_eq!(records[0]["tags"], serde_json::json!(["verify"]));
    assert_eq!(records[0]["source"]["session_id"], "session-cli");
    assert_eq!(records[0]["source"]["turn_id"], "turn-cli");
    assert_eq!(records[0]["source_uri"], "codel00p://sessions/session-cli");
}

#[test]
fn memory_search_filters_sensitive_records_explicitly() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-normal",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
    );
    seed_candidate_with_sensitivity(
        &db_path,
        "mem-sensitive",
        MemoryKind::Workflow,
        "Use the private verify credential only from CI.",
        "verify",
        MemorySensitivity::Sensitive,
    );
    approve_candidate(&db_path, "mem-normal", "alice");
    approve_candidate(&db_path, "mem-sensitive", "alice");

    let default_output = run_codel00p(&db_path, &["memory", "search", "--text", "verify"]);
    let sensitive_output = run_codel00p(
        &db_path,
        &[
            "memory",
            "search",
            "--text",
            "verify",
            "--sensitivity",
            "sensitive",
            "--json",
        ],
    );

    assert!(
        default_output.status.success(),
        "stderr: {}",
        stderr(&default_output)
    );
    assert!(
        sensitive_output.status.success(),
        "stderr: {}",
        stderr(&sensitive_output)
    );
    assert_eq!(
        stdout(&default_output),
        "mem-normal\tapproved\tworkflow\tmatched text verify\tRun pnpm verify before pushing main.\n"
    );
    let records: serde_json::Value =
        serde_json::from_str(&stdout(&sensitive_output)).expect("search json");
    let records = records.as_array().expect("record array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "mem-sensitive");
    assert_eq!(records[0]["sensitivity"], "sensitive");
    assert_eq!(
        records[0]["reason"],
        "matched text verify and sensitivity sensitive"
    );
}
