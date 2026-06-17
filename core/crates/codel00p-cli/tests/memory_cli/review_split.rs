use super::support::*;

#[test]
fn memory_split_creates_candidate_inherits_metadata_and_audits_both() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-source",
        MemoryKind::Convention,
        "Use cargo from core/. Run tests serially.",
        "cargo",
    );
    approve_candidate(&db_path, "mem-source", "alice");

    let split = run_codel00p(
        &db_path,
        &[
            "memory",
            "split",
            "mem-source",
            "mem-new",
            "--content",
            "Run tests serially.",
            "--actor",
            "alice",
            "--reason",
            "split testing note",
            "--json",
        ],
    );

    assert!(split.status.success(), "stderr: {}", stderr(&split));

    // The command returns the newly created candidate memory.
    let new_mem: serde_json::Value = serde_json::from_str(&stdout(&split)).expect("split json");
    assert_eq!(new_mem["id"], "mem-new");
    assert_eq!(new_mem["status"], "candidate");
    assert_eq!(new_mem["content"], "Run tests serially.");
    // New memory inherits tags from the source.
    assert_eq!(new_mem["tags"], serde_json::json!(["cargo"]));

    // Source memory remains approved.
    let source_show = run_codel00p(&db_path, &["memory", "show", "mem-source"]);
    assert!(stdout(&source_show).contains("status: approved"));

    // Source audit log records the split with split_into pointer.
    let source_audit = run_codel00p(&db_path, &["memory", "audit", "mem-source", "--json"]);
    let events: serde_json::Value =
        serde_json::from_str(&stdout(&source_audit)).expect("audit json");
    let split_event = events
        .as_array()
        .expect("audit array")
        .iter()
        .find(|event| event["action"] == "split")
        .expect("split event present");
    assert_eq!(split_event["actor"], "alice");
    assert_eq!(split_event["split_into"], "mem-new");
    assert_eq!(split_event["reason"], "split testing note");
}

#[test]
fn memory_split_with_source_content_update() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-source",
        MemoryKind::Convention,
        "Use cargo from core/. Run tests serially.",
        "cargo",
    );
    approve_candidate(&db_path, "mem-source", "alice");

    let split = run_codel00p(
        &db_path,
        &[
            "memory",
            "split",
            "mem-source",
            "mem-new",
            "--content",
            "Run tests serially.",
            "--source-content",
            "Use cargo from core/.",
            "--actor",
            "alice",
            "--json",
        ],
    );

    assert!(split.status.success(), "stderr: {}", stderr(&split));

    // Source has updated content.
    let source_show = run_codel00p(&db_path, &["memory", "show", "mem-source"]);
    assert!(stdout(&source_show).contains("Use cargo from core/."));
    // Source content no longer contains the split-off part.
    assert!(!stdout(&source_show).contains("Run tests serially."));
}

#[test]
fn memory_split_new_id_collision_fails() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-source",
        MemoryKind::Convention,
        "A reusable convention.",
        "tag",
    );
    approve_candidate(&db_path, "mem-source", "alice");
    seed_candidate(
        &db_path,
        "mem-existing",
        MemoryKind::Convention,
        "Already exists.",
        "tag",
    );

    let split = run_codel00p(
        &db_path,
        &[
            "memory",
            "split",
            "mem-source",
            "mem-existing",
            "--content",
            "Part of the convention.",
            "--actor",
            "alice",
        ],
    );

    assert!(!split.status.success());
    assert!(
        stderr(&split).contains("already exists") || stderr(&split).contains("MemoryAlreadyExists")
    );
}
