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
fn merge_archives_the_source_enriches_the_target_and_audits_both() {
    let mut store = InMemoryMemoryStore::default();
    seed_approved(
        &mut store,
        "dup",
        "Run cargo from core.",
        &["dup-tag", "shared"],
    );
    seed_approved(
        &mut store,
        "keep",
        "Run cargo commands from core.",
        &["shared", "keep-tag"],
    );

    let target = store
        .merge(
            "dup",
            "keep",
            MemoryMerge::new("alice").with_reason("near-duplicate"),
        )
        .expect("merge duplicate into survivor");

    // Target survives, active, with the union of both tag sets (order preserved,
    // no duplicates).
    assert_eq!(target.entry().id(), "keep");
    assert_eq!(target.entry().status(), MemoryStatus::Approved);
    assert_eq!(target.entry().tags(), ["shared", "keep-tag", "dup-tag"]);

    // Source is archived.
    let source = store.get("dup").expect("load source");
    assert_eq!(source.entry().status(), MemoryStatus::Archived);

    // Source audit log records the merge with the target reference.
    let source_audit = store.audit_log("dup").expect("source audit");
    let merged = source_audit
        .iter()
        .find(|event| event.action() == MemoryAuditAction::Merged)
        .expect("source has a merged event");
    assert_eq!(merged.actor(), "alice");
    assert_eq!(merged.merged_into(), Some("keep"));
    assert_eq!(merged.reason(), Some("near-duplicate"));

    // Target audit log records the absorption, without a merged_into pointer.
    let target_audit = store.audit_log("keep").expect("target audit");
    let absorbed = target_audit
        .iter()
        .find(|event| event.action() == MemoryAuditAction::Merged)
        .expect("target has a merged event");
    assert_eq!(absorbed.merged_into(), None);
    assert_eq!(absorbed.reason(), Some("absorbed dup: near-duplicate"));
}

#[test]
fn merge_into_self_is_rejected() {
    let mut store = InMemoryMemoryStore::default();
    seed_approved(&mut store, "mem", "A reusable convention.", &[]);

    let error = store
        .merge("mem", "mem", MemoryMerge::new("alice"))
        .expect_err("self-merge must fail");
    assert!(matches!(error, MemoryError::InvalidMerge { .. }));
    assert_eq!(
        store.get("mem").expect("still there").entry().status(),
        MemoryStatus::Approved
    );
}

#[test]
fn merge_across_projects_is_rejected() {
    let mut store = InMemoryMemoryStore::default();
    seed_approved(&mut store, "here", "Belongs to project one.", &[]);
    // A candidate in a different project.
    store
        .create_candidate(MemoryCandidateInput::new(
            "there",
            ProjectRef::new("project-2", "other"),
            MemoryKind::Convention,
            "Belongs to project two.",
            source(),
        ))
        .expect("create cross-project candidate");

    let error = store
        .merge("here", "there", MemoryMerge::new("alice"))
        .expect_err("cross-project merge must fail");
    assert!(matches!(error, MemoryError::InvalidMerge { .. }));
}

#[test]
fn merge_requires_both_memories_active() {
    let mut store = InMemoryMemoryStore::default();
    seed_approved(&mut store, "active", "Still active.", &[]);
    // Archive a second memory so it is inactive.
    seed_approved(&mut store, "archived", "No longer active.", &[]);
    store
        .review("archived", ReviewDecision::archive("bob", "obsolete"))
        .expect("archive");

    // Inactive target is rejected.
    let into_archived = store
        .merge("active", "archived", MemoryMerge::new("alice"))
        .expect_err("merging into an archived target must fail");
    assert!(matches!(into_archived, MemoryError::InvalidMerge { .. }));

    // Inactive source is rejected.
    let from_archived = store
        .merge("archived", "active", MemoryMerge::new("alice"))
        .expect_err("merging from an archived source must fail");
    assert!(matches!(from_archived, MemoryError::InvalidMerge { .. }));

    // Nothing changed: the active memory is untouched.
    assert_eq!(
        store.get("active").expect("active").entry().status(),
        MemoryStatus::Approved
    );
}

#[test]
fn merge_returns_not_found_for_unknown_ids() {
    let mut store = InMemoryMemoryStore::default();
    seed_approved(&mut store, "real", "A real memory.", &[]);

    let error = store
        .merge("real", "ghost", MemoryMerge::new("alice"))
        .expect_err("unknown target must fail");
    assert!(matches!(error, MemoryError::MemoryNotFound { .. }));
}
