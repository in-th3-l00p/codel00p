use super::support::*;

#[test]
fn memory_quality_lists_low_quality_active_memory() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-vague",
        MemoryKind::Decision,
        "This is important.",
        "review",
    );
    seed_candidate(
        &db_path,
        "mem-strong",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main after editing provider policy.",
        "verify",
    );
    seed_candidate(
        &db_path,
        "mem-rejected",
        MemoryKind::Decision,
        "That thing matters.",
        "review",
    );
    let reject = run_codel00p(
        &db_path,
        &[
            "memory",
            "reject",
            "mem-rejected",
            "--actor",
            "alice",
            "--reason",
            "too vague",
        ],
    );
    assert!(reject.status.success(), "stderr: {}", stderr(&reject));

    let output = run_codel00p(
        &db_path,
        &["memory", "quality", "--max-score", "80", "--limit", "5"],
    );
    let output_json = run_codel00p(
        &db_path,
        &[
            "memory",
            "quality",
            "--max-score",
            "80",
            "--limit",
            "5",
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
        "mem-vague\tcandidate\tdecision\t65\tThis is important.\n"
    );
    let records: serde_json::Value =
        serde_json::from_str(&stdout(&output_json)).expect("quality json");
    let records = records.as_array().expect("record array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "mem-vague");
    assert_eq!(records[0]["quality"]["score"], 65);
    assert_eq!(
        records[0]["quality"]["findings"],
        serde_json::json!([
            "content is too short to be reusable",
            "content uses vague language"
        ])
    );
}

#[test]
fn memory_quality_filters_low_quality_memory_by_status() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-low-quality-candidate",
        MemoryKind::Workflow,
        "Run tests.",
        "verify",
    );
    seed_candidate(
        &db_path,
        "mem-low-quality-approved",
        MemoryKind::Workflow,
        "Use credential.",
        "credential",
    );
    approve_candidate(&db_path, "mem-low-quality-approved", "alice");

    let output = run_codel00p(
        &db_path,
        &[
            "memory",
            "quality",
            "--status",
            "approved",
            "--max-score",
            "80",
            "--json",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let records: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("quality json");
    let records = records.as_array().expect("record array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "mem-low-quality-approved");
    assert_eq!(records[0]["status"], "approved");
}

#[test]
fn memory_quality_filters_low_quality_memory_by_kind() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-vague-decision",
        MemoryKind::Decision,
        "This is important.",
        "review",
    );
    seed_candidate(
        &db_path,
        "mem-short-workflow",
        MemoryKind::Workflow,
        "Run tests.",
        "verify",
    );

    let output = run_codel00p(
        &db_path,
        &[
            "memory",
            "quality",
            "--kind",
            "workflow",
            "--max-score",
            "80",
            "--json",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let records: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("quality json");
    let records = records.as_array().expect("record array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "mem-short-workflow");
    assert_eq!(records[0]["kind"], "workflow");
}

#[test]
fn memory_quality_filters_low_quality_memory_by_sensitivity() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-vague-normal",
        MemoryKind::Workflow,
        "Run tests.",
        "verify",
    );
    seed_candidate_with_sensitivity(
        &db_path,
        "mem-vague-sensitive",
        MemoryKind::Workflow,
        "Use credential.",
        "credential",
        MemorySensitivity::Sensitive,
    );

    let output = run_codel00p(
        &db_path,
        &[
            "memory",
            "quality",
            "--sensitivity",
            "sensitive",
            "--max-score",
            "80",
            "--json",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let records: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("quality json");
    let records = records.as_array().expect("record array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "mem-vague-sensitive");
    assert_eq!(records[0]["sensitivity"], "sensitive");
}

#[test]
fn memory_quality_filters_low_quality_memory_by_tag() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-vague-credential",
        MemoryKind::Workflow,
        "Use credential.",
        "credential",
    );
    seed_candidate(
        &db_path,
        "mem-vague-verify",
        MemoryKind::Workflow,
        "Run tests.",
        "verify",
    );

    let output = run_codel00p(
        &db_path,
        &[
            "memory",
            "quality",
            "--tag",
            "credential",
            "--max-score",
            "80",
            "--json",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let records: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("quality json");
    let records = records.as_array().expect("record array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "mem-vague-credential");
    assert_eq!(records[0]["tags"], serde_json::json!(["credential"]));
}
