use super::support::*;

/// Seeds three approved memories — two near-duplicates of the same kind plus an
/// unrelated one — then drives `memory curate` through dry-run, JSON, and apply.
fn seed_three(db_path: &std::path::Path) {
    seed_candidate(
        db_path,
        "mem-a-keep",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
    );
    seed_candidate(
        db_path,
        "mem-b-dup",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing to main branch.",
        "verify",
    );
    seed_candidate(
        db_path,
        "mem-z-other",
        MemoryKind::Workflow,
        "The harness owns tool execution.",
        "harness",
    );
    approve_candidate(db_path, "mem-a-keep", "alice");
    approve_candidate(db_path, "mem-b-dup", "alice");
    approve_candidate(db_path, "mem-z-other", "alice");
}

#[test]
fn memory_curate_dry_run_lists_clusters_without_archiving() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_three(&db_path);

    let output = run_codel00p(&db_path, &["memory", "curate"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    // The richer memory (more unique tokens → higher quality) survives.
    assert!(text.contains("keep\tmem-b-dup\tworkflow"), "missing survivor line: {text}");
    assert!(text.contains("archive\tmem-a-keep\tworkflow"), "missing duplicate line: {text}");
    assert!(!text.contains("mem-z-other"), "unrelated memory should not appear: {text}");
    assert!(text.contains("1 cluster(s), 1 duplicate(s)"), "missing summary: {text}");

    // Dry-run must NOT mutate: the duplicate is still approved.
    let approved = run_codel00p(&db_path, &["memory", "list", "--status", "approved"]);
    assert!(stdout(&approved).contains("mem-a-keep"), "dry-run archived the duplicate");
}

#[test]
fn memory_curate_json_reports_survivor_and_duplicates() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_three(&db_path);

    let output = run_codel00p(&db_path, &["memory", "curate", "--json"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let plan: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("curate json");
    assert_eq!(plan["applied"], false);
    let clusters = plan["clusters"].as_array().expect("clusters array");
    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0]["survivor"]["id"], "mem-b-dup");
    let duplicates = clusters[0]["duplicates"].as_array().expect("duplicates array");
    assert_eq!(duplicates.len(), 1);
    assert_eq!(duplicates[0]["id"], "mem-a-keep");
    assert_eq!(duplicates[0]["similarity"], 83);
}

#[test]
fn memory_curate_apply_archives_duplicates_and_keeps_survivor() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_three(&db_path);

    let output = run_codel00p(&db_path, &["memory", "curate", "--apply"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("Archived 1 duplicate memory record(s) across 1 cluster(s)"),
        "unexpected apply output: {}",
        stdout(&output)
    );

    // The duplicate is archived; the survivor and the unrelated memory stay approved.
    let archived = run_codel00p(&db_path, &["memory", "list", "--status", "archived"]);
    assert!(stdout(&archived).contains("mem-a-keep"), "duplicate not archived");

    let approved = run_codel00p(&db_path, &["memory", "list", "--status", "approved"]);
    let approved_text = stdout(&approved);
    assert!(approved_text.contains("mem-b-dup"), "survivor should remain approved");
    assert!(approved_text.contains("mem-z-other"), "unrelated memory should remain approved");
    assert!(!approved_text.contains("mem-a-keep"), "duplicate should no longer be approved");

    // Re-running finds nothing left to consolidate.
    let again = run_codel00p(&db_path, &["memory", "curate"]);
    assert!(
        stdout(&again).contains("No near-duplicate memories to consolidate"),
        "second pass should be a no-op: {}",
        stdout(&again)
    );
}
