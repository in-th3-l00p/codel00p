use super::support::*;

/// Seeds a candidate, optionally tagged, and approves it so it is active.
fn seed_approved(store: &mut InMemoryMemoryStore, id: &str, content: &str, tags: &[&str]) {
    let mut input =
        MemoryCandidateInput::new(id, project(), MemoryKind::Convention, content, source());
    for tag in tags {
        input = input.with_tag(*tag);
    }
    store.create_candidate(input).expect("create candidate");
    store
        .review(id, ReviewDecision::approve("alice"))
        .expect("approve candidate");
}

#[test]
fn split_creates_new_candidate_carries_metadata_and_audits_both_sides() {
    let mut store = InMemoryMemoryStore::default();
    seed_approved(
        &mut store,
        "source",
        "Use cargo from core/. Tests go in lifecycle/. Run serially.",
        &["cargo", "testing"],
    );

    let new_record = store
        .split(
            "source",
            MemorySplit::new("alice", "new-mem", "Run tests serially.").with_reason("too broad"),
        )
        .expect("split source memory");

    // New memory is a candidate with the split-off content.
    assert_eq!(new_record.entry().id(), "new-mem");
    assert_eq!(new_record.entry().status(), MemoryStatus::Candidate);
    assert_eq!(new_record.entry().content(), "Run tests serially.");

    // New memory inherits project, kind, sensitivity, and tags from source.
    assert_eq!(new_record.entry().project().id(), "project-1");
    assert_eq!(new_record.entry().kind(), MemoryKind::Convention);
    assert_eq!(new_record.entry().sensitivity(), MemorySensitivity::Normal);
    assert_eq!(new_record.entry().tags(), ["cargo", "testing"]);

    // Source memory remains active (unchanged content).
    let source = store.get("source").expect("load source");
    assert_eq!(source.entry().status(), MemoryStatus::Approved);
    assert_eq!(
        source.entry().content(),
        "Use cargo from core/. Tests go in lifecycle/. Run serially."
    );

    // Source audit log records the split with split_into pointer.
    let source_audit = store.audit_log("source").expect("source audit");
    let split_event = source_audit
        .iter()
        .find(|event| event.action() == MemoryAuditAction::Split)
        .expect("source has a split event");
    assert_eq!(split_event.actor(), "alice");
    assert_eq!(split_event.split_into(), Some("new-mem"));
    assert_eq!(split_event.reason(), Some("too broad"));

    // New memory audit log records the split_from event.
    let new_audit = store.audit_log("new-mem").expect("new memory audit");
    let split_from_event = new_audit
        .iter()
        .find(|event| event.action() == MemoryAuditAction::Split)
        .expect("new memory has a split event");
    assert_eq!(split_from_event.split_into(), None);
    assert_eq!(
        split_from_event.reason(),
        Some("split from source: too broad")
    );
}

#[test]
fn split_with_updated_source_content_keeps_source_active_with_new_content() {
    let mut store = InMemoryMemoryStore::default();
    seed_approved(
        &mut store,
        "source",
        "Use cargo from core/. Tests go in lifecycle/. Run serially.",
        &[],
    );

    store
        .split(
            "source",
            MemorySplit::new("alice", "new-mem", "Run tests serially.")
                .with_updated_source_content("Use cargo from core/.")
                .with_reason("split testing note"),
        )
        .expect("split with updated source");

    // Source retains its active status but now carries the trimmed content.
    let source = store.get("source").expect("load source");
    assert_eq!(source.entry().status(), MemoryStatus::Approved);
    assert_eq!(source.entry().content(), "Use cargo from core/.");

    // New candidate has the split-off content.
    let new_mem = store.get("new-mem").expect("load new memory");
    assert_eq!(new_mem.entry().content(), "Run tests serially.");
}

#[test]
fn split_new_id_collision_is_rejected() {
    let mut store = InMemoryMemoryStore::default();
    seed_approved(&mut store, "source", "Some convention.", &[]);
    seed_approved(&mut store, "existing", "Already exists.", &[]);

    let error = store
        .split(
            "source",
            MemorySplit::new("alice", "existing", "Part of the convention."),
        )
        .expect_err("collision on new_id must fail");
    assert!(matches!(error, MemoryError::MemoryAlreadyExists { .. }));

    // Source is untouched.
    assert_eq!(
        store.get("source").expect("source").entry().status(),
        MemoryStatus::Approved
    );
}

#[test]
fn split_unknown_source_is_rejected() {
    let mut store = InMemoryMemoryStore::default();

    let error = store
        .split(
            "ghost",
            MemorySplit::new("alice", "new-mem", "Some content."),
        )
        .expect_err("unknown source must fail");
    assert!(matches!(error, MemoryError::MemoryNotFound { .. }));
}

#[test]
fn split_inactive_source_is_rejected() {
    let mut store = InMemoryMemoryStore::default();
    seed_approved(&mut store, "archived-mem", "Some convention.", &[]);
    store
        .review("archived-mem", ReviewDecision::archive("bob", "obsolete"))
        .expect("archive");

    let error = store
        .split(
            "archived-mem",
            MemorySplit::new("alice", "new-mem", "Part of it."),
        )
        .expect_err("inactive source must fail");
    assert!(matches!(error, MemoryError::InvalidSplit { .. }));
}

#[test]
fn split_without_reason_records_split_from_without_reason_prefix() {
    let mut store = InMemoryMemoryStore::default();
    seed_approved(&mut store, "source", "Convention text.", &[]);

    store
        .split(
            "source",
            MemorySplit::new("alice", "new-mem", "Part of it."),
        )
        .expect("split without reason");

    let new_audit = store.audit_log("new-mem").expect("new memory audit");
    let split_from = new_audit
        .iter()
        .find(|event| event.action() == MemoryAuditAction::Split)
        .expect("split event");
    assert_eq!(split_from.reason(), Some("split from source"));
}
