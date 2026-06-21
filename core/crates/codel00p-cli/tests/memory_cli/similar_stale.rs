use super::support::*;

#[test]
fn memory_similar_scores_active_near_duplicates() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-original",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
    );
    seed_candidate(
        &db_path,
        "mem-unrelated",
        MemoryKind::Workflow,
        "The harness owns tool execution.",
        "harness",
    );
    seed_candidate(
        &db_path,
        "mem-archived",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main branch.",
        "verify",
    );
    approve_candidate(&db_path, "mem-original", "alice");
    approve_candidate(&db_path, "mem-archived", "alice");
    archive_memory(&db_path, "mem-archived", "alice", "superseded");

    let output = run_codel00p(
        &db_path,
        &[
            "memory",
            "similar",
            "--kind",
            "workflow",
            "--content",
            "Run pnpm verify before pushing to main branch.",
            "--threshold",
            "70",
        ],
    );
    let output_json = run_codel00p(
        &db_path,
        &[
            "memory",
            "similar",
            "--kind",
            "workflow",
            "--content",
            "Run pnpm verify before pushing to main branch.",
            "--threshold",
            "70",
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
        // Shingle (token-bigram) Jaccard scores the reworded duplicate at 83
        // (was 75 under unigram Jaccard).
        "mem-original\tapproved\tworkflow\t83\tRun pnpm verify before pushing main.\n"
    );
    let records: serde_json::Value =
        serde_json::from_str(&stdout(&output_json)).expect("similar json");
    let records = records.as_array().expect("record array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "mem-original");
    assert_eq!(records[0]["status"], "approved");
    assert_eq!(records[0]["kind"], "workflow");
    assert_eq!(records[0]["score"], 83);
    assert_eq!(
        records[0]["content"],
        "Run pnpm verify before pushing main."
    );
    assert_eq!(records[0]["tags"], serde_json::json!(["verify"]));
    assert_eq!(records[0]["source_uri"], "codel00p://sessions/session-cli");
}

#[test]
fn memory_stale_lists_superseded_approved_memory() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-original",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
    );
    approve_candidate(&db_path, "mem-original", "alice");
    seed_candidate(
        &db_path,
        "mem-unrelated",
        MemoryKind::Workflow,
        "The harness owns tool execution.",
        "harness",
    );
    seed_candidate(
        &db_path,
        "mem-archived",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing to main branch.",
        "verify",
    );
    approve_candidate(&db_path, "mem-archived", "alice");
    archive_memory(&db_path, "mem-archived", "alice", "superseded");
    seed_candidate(
        &db_path,
        "mem-newer",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing to main branch.",
        "verify",
    );

    let output = run_codel00p(
        &db_path,
        &["memory", "stale", "--kind", "workflow", "--threshold", "70"],
    );
    let output_json = run_codel00p(
        &db_path,
        &[
            "memory",
            "stale",
            "--kind",
            "workflow",
            "--threshold",
            "70",
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
        "mem-original\tapproved\tworkflow\t83\tmem-newer\tRun pnpm verify before pushing main.\n"
    );
    let records: serde_json::Value =
        serde_json::from_str(&stdout(&output_json)).expect("stale json");
    let records = records.as_array().expect("record array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "mem-original");
    assert_eq!(records[0]["status"], "approved");
    assert_eq!(records[0]["kind"], "workflow");
    assert_eq!(records[0]["score"], 83);
    assert_eq!(records[0]["newer"]["id"], "mem-newer");
    assert_eq!(records[0]["newer"]["status"], "candidate");
    assert_eq!(
        records[0]["newer"]["content"],
        "Run pnpm verify before pushing to main branch."
    );
    assert_eq!(
        records[0]["newer"]["source_uri"],
        "codel00p://sessions/session-cli"
    );
}
