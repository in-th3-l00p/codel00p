use super::support::*;

#[test]
fn rejects_empty_memory_edit_content() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-1",
            project(),
            MemoryKind::Workflow,
            "Run tests before pushing.",
            source(),
        ))
        .expect("create candidate");

    let error = store
        .edit("mem-1", MemoryEdit::replace_content("alice", " "))
        .expect_err("empty edit content must fail");
    let record = store.get("mem-1").expect("load memory after failed edit");
    let audit = store.audit_log("mem-1").expect("audit log");

    assert!(matches!(error, MemoryError::InvalidEdit { .. }));
    assert_eq!(record.entry().content(), "Run tests before pushing.");
    assert_eq!(audit.len(), 1);
}

#[test]
fn approve_reject_and_archive_are_explicit_lifecycle_transitions() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-1",
            project(),
            MemoryKind::Architecture,
            "The harness owns tool execution.",
            source(),
        ))
        .expect("create candidate");

    let approved = store
        .review("mem-1", ReviewDecision::approve("alice"))
        .expect("approve candidate");
    let archived = store
        .review(
            "mem-1",
            ReviewDecision::archive("bob", "replaced by newer memory"),
        )
        .expect("archive approved memory");

    assert_eq!(approved.entry().status(), MemoryStatus::Approved);
    assert_eq!(archived.entry().status(), MemoryStatus::Archived);
}

#[test]
fn invalid_lifecycle_transitions_fail() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-1",
            project(),
            MemoryKind::Architecture,
            "The harness owns tool execution.",
            source(),
        ))
        .expect("create candidate");
    store
        .review("mem-1", ReviewDecision::reject("alice", "too vague"))
        .expect("reject candidate");

    let error = store
        .review("mem-1", ReviewDecision::approve("bob"))
        .expect_err("rejected memory cannot be approved");

    assert!(matches!(error, MemoryError::InvalidTransition { .. }));
}

#[test]
fn lifecycle_changes_are_audited_in_order() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-1",
            project(),
            MemoryKind::Architecture,
            "The harness owns tool execution.",
            source(),
        ))
        .expect("create candidate");
    store
        .review("mem-1", ReviewDecision::approve("alice"))
        .expect("approve candidate");

    let audit = store.audit_log("mem-1").expect("audit log");

    assert_eq!(audit.len(), 2);
    assert_eq!(audit[0].sequence(), 1);
    assert_eq!(audit[0].action(), MemoryAuditAction::CandidateCreated);
    assert_eq!(audit[1].sequence(), 2);
    assert_eq!(audit[1].actor(), "alice");
    assert_eq!(audit[1].action(), MemoryAuditAction::Approved);
}

#[test]
fn memory_edit_updates_content_and_audits_revision() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(
            MemoryCandidateInput::new(
                "mem-1",
                project(),
                MemoryKind::Workflow,
                "Run tests before pushing.",
                source(),
            )
            .with_tag("verify"),
        )
        .expect("create candidate");
    store
        .review("mem-1", ReviewDecision::approve("alice"))
        .expect("approve candidate");

    let edited = store
        .edit(
            "mem-1",
            MemoryEdit::replace_content("bob", "Run pnpm verify before pushing main.")
                .with_reason("clarified verification command"),
        )
        .expect("edit memory");
    let audit = store.audit_log("mem-1").expect("audit log");
    let edit_event = serde_json::to_value(&audit[2]).expect("serialize edit audit");

    assert_eq!(edited.entry().status(), MemoryStatus::Approved);
    assert_eq!(
        edited.entry().content(),
        "Run pnpm verify before pushing main."
    );
    assert_eq!(edited.entry().source(), Some(&source()));
    assert_eq!(edited.entry().tags(), ["verify"]);
    assert_eq!(audit.len(), 3);
    assert_eq!(audit[2].sequence(), 3);
    assert_eq!(audit[2].action(), MemoryAuditAction::Edited);
    assert_eq!(audit[2].actor(), "bob");
    assert_eq!(audit[2].reason(), Some("clarified verification command"));
    assert_eq!(edit_event["previous_content"], "Run tests before pushing.");
    assert_eq!(
        edit_event["new_content"],
        "Run pnpm verify before pushing main."
    );
}
