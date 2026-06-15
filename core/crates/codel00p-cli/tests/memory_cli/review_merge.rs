use super::support::*;

#[test]
fn memory_merge_archives_source_enriches_target_and_audits() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-dup",
        MemoryKind::Convention,
        "Run cargo from core.",
        "dup-tag",
    );
    seed_candidate(
        &db_path,
        "mem-keep",
        MemoryKind::Convention,
        "Run cargo commands from core.",
        "keep-tag",
    );
    approve_candidate(&db_path, "mem-dup", "alice");
    approve_candidate(&db_path, "mem-keep", "alice");

    let merge = run_codel00p(
        &db_path,
        &[
            "memory",
            "merge",
            "mem-dup",
            "mem-keep",
            "--actor",
            "alice",
            "--reason",
            "near-duplicate",
            "--json",
        ],
    );
    let target_show = run_codel00p(&db_path, &["memory", "show", "mem-keep"]);
    let source_show = run_codel00p(&db_path, &["memory", "show", "mem-dup"]);
    let source_audit = run_codel00p(&db_path, &["memory", "audit", "mem-dup", "--json"]);

    assert!(merge.status.success(), "stderr: {}", stderr(&merge));

    // The command returns the enriched, still-approved survivor.
    let survivor: serde_json::Value = serde_json::from_str(&stdout(&merge)).expect("merge json");
    assert_eq!(survivor["id"], "mem-keep");
    assert_eq!(survivor["status"], "approved");
    assert_eq!(survivor["tags"], serde_json::json!(["keep-tag", "dup-tag"]));

    // The survivor persisted its enriched tags; the duplicate is archived.
    assert!(stdout(&target_show).contains("tags: keep-tag,dup-tag"));
    assert!(stdout(&source_show).contains("status: archived"));

    // The duplicate's audit trail records the merge and its target.
    let events: serde_json::Value =
        serde_json::from_str(&stdout(&source_audit)).expect("audit json");
    let merged = events
        .as_array()
        .expect("audit array")
        .iter()
        .find(|event| event["action"] == "merged")
        .expect("merged event present");
    assert_eq!(merged["actor"], "alice");
    assert_eq!(merged["merged_into"], "mem-keep");
    assert_eq!(merged["reason"], "near-duplicate");
}

#[test]
fn memory_merge_into_self_fails() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-keep",
        MemoryKind::Convention,
        "A reusable convention.",
        "keep-tag",
    );
    approve_candidate(&db_path, "mem-keep", "alice");

    let merge = run_codel00p(
        &db_path,
        &[
            "memory", "merge", "mem-keep", "mem-keep", "--actor", "alice",
        ],
    );

    assert!(!merge.status.success());
    assert!(stderr(&merge).contains("merge"));
}
