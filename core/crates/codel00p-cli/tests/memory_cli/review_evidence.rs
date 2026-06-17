use super::support::*;

#[test]
fn memory_evidence_add_appends_link_and_surfaces_in_json() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-decision",
        MemoryKind::Decision,
        "Adopted axum for the cloud service.",
        "cloud",
    );

    let add = run_codel00p(
        &db_path,
        &[
            "memory",
            "evidence",
            "add",
            "mem-decision",
            "https://github.com/acme/repo/pull/12",
            "--kind",
            "pr",
            "--note",
            "decision PR",
            "--actor",
            "alice",
            "--reason",
            "link the decision",
            "--json",
        ],
    );
    assert!(add.status.success(), "stderr: {}", stderr(&add));
    let record: serde_json::Value = serde_json::from_str(&stdout(&add)).expect("add json");
    assert_eq!(record["id"], "mem-decision");
    assert_eq!(record["evidence"][0]["kind"], "pr");
    assert_eq!(
        record["evidence"][0]["reference"],
        "https://github.com/acme/repo/pull/12"
    );
    assert_eq!(record["evidence"][0]["note"], "decision PR");

    // show --json surfaces the evidence too.
    let show = run_codel00p(&db_path, &["memory", "show", "mem-decision", "--json"]);
    assert!(show.status.success(), "stderr: {}", stderr(&show));
    let shown: serde_json::Value = serde_json::from_str(&stdout(&show)).expect("show json");
    assert_eq!(shown["evidence"][0]["kind"], "pr");

    // show text output includes an evidence line.
    let show_text = run_codel00p(&db_path, &["memory", "show", "mem-decision"]);
    assert!(
        stdout(&show_text)
            .contains("evidence: pr https://github.com/acme/repo/pull/12 (decision PR)")
    );

    // audit records the evidence_added event with the reference.
    let audit = run_codel00p(&db_path, &["memory", "audit", "mem-decision", "--json"]);
    assert!(audit.status.success(), "stderr: {}", stderr(&audit));
    let events: serde_json::Value = serde_json::from_str(&stdout(&audit)).expect("audit json");
    let last = events
        .as_array()
        .expect("events array")
        .last()
        .expect("event");
    assert_eq!(last["action"], "evidence_added");
    assert_eq!(last["actor"], "alice");
    assert_eq!(last["reason"], "link the decision");
    assert_eq!(
        last["evidence_reference"],
        "https://github.com/acme/repo/pull/12"
    );
}

#[test]
fn memory_evidence_add_defaults_kind_to_other() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-decision",
        MemoryKind::Decision,
        "Adopted axum for the cloud service.",
        "cloud",
    );

    let add = run_codel00p(
        &db_path,
        &[
            "memory",
            "evidence",
            "add",
            "mem-decision",
            "some-reference",
            "--actor",
            "alice",
            "--json",
        ],
    );
    assert!(add.status.success(), "stderr: {}", stderr(&add));
    let record: serde_json::Value = serde_json::from_str(&stdout(&add)).expect("add json");
    assert_eq!(record["evidence"][0]["kind"], "other");
    assert_eq!(record["evidence"][0]["reference"], "some-reference");
    assert!(record["evidence"][0].get("note").is_none());
}
