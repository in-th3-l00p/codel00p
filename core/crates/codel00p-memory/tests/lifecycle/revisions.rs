use super::support::*;

#[test]
fn revisions_starts_with_initial_content_from_candidate_created() {
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

    let revisions = store.revisions("mem-1").expect("revisions");

    assert_eq!(revisions.len(), 1);
    assert_eq!(revisions[0].revision, 1);
    assert_eq!(revisions[0].content, "Run tests before pushing.");
    assert_eq!(revisions[0].actor, "system");
    assert_eq!(revisions[0].action, MemoryAuditAction::CandidateCreated);
    assert_eq!(revisions[0].reason, None);
}

#[test]
fn revisions_grows_with_each_edit() {
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
    store
        .review("mem-1", ReviewDecision::approve("alice"))
        .expect("approve");

    store
        .edit(
            "mem-1",
            MemoryEdit::replace_content("bob", "Run pnpm verify before pushing main.")
                .with_reason("clarified command"),
        )
        .expect("first edit");
    store
        .edit(
            "mem-1",
            MemoryEdit::replace_content("carol", "Run pnpm verify && pnpm test before pushing.")
                .with_reason("added test step"),
        )
        .expect("second edit");

    let revisions = store.revisions("mem-1").expect("revisions");

    assert_eq!(revisions.len(), 3);

    assert_eq!(revisions[0].revision, 1);
    assert_eq!(revisions[0].content, "Run tests before pushing.");
    assert_eq!(revisions[0].actor, "system");
    assert_eq!(revisions[0].action, MemoryAuditAction::CandidateCreated);
    assert_eq!(revisions[0].reason, None);

    assert_eq!(revisions[1].revision, 2);
    assert_eq!(revisions[1].content, "Run pnpm verify before pushing main.");
    assert_eq!(revisions[1].actor, "bob");
    assert_eq!(revisions[1].action, MemoryAuditAction::Edited);
    assert_eq!(revisions[1].reason, Some("clarified command".to_string()));

    assert_eq!(revisions[2].revision, 3);
    assert_eq!(
        revisions[2].content,
        "Run pnpm verify && pnpm test before pushing."
    );
    assert_eq!(revisions[2].actor, "carol");
    assert_eq!(revisions[2].action, MemoryAuditAction::Edited);
    assert_eq!(revisions[2].reason, Some("added test step".to_string()));
}

#[test]
fn revisions_includes_restore_as_another_edit_revision() {
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
    store
        .review("mem-1", ReviewDecision::approve("alice"))
        .expect("approve");
    store
        .edit(
            "mem-1",
            MemoryEdit::replace_content("bob", "Run pnpm verify before pushing main.")
                .with_reason("clarified command"),
        )
        .expect("edit");

    // Restore: re-edit with previous content (simulates restore operation)
    store
        .edit(
            "mem-1",
            MemoryEdit::replace_content("carol", "Run tests before pushing.")
                .with_reason("undo edit"),
        )
        .expect("restore via edit");

    let revisions = store.revisions("mem-1").expect("revisions");

    assert_eq!(revisions.len(), 3);
    assert_eq!(revisions[0].revision, 1);
    assert_eq!(revisions[0].content, "Run tests before pushing.");
    assert_eq!(revisions[1].revision, 2);
    assert_eq!(revisions[1].content, "Run pnpm verify before pushing main.");
    assert_eq!(revisions[2].revision, 3);
    assert_eq!(revisions[2].content, "Run tests before pushing.");
    assert_eq!(revisions[2].actor, "carol");
    assert_eq!(revisions[2].reason, Some("undo edit".to_string()));
}

#[test]
fn revisions_sequences_match_audit_sequences_of_content_events() {
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
        .expect("approve");
    store
        .edit(
            "mem-1",
            MemoryEdit::replace_content("bob", "The harness owns tool execution and streaming."),
        )
        .expect("edit");

    let audit = store.audit_log("mem-1").expect("audit log");
    let revisions = store.revisions("mem-1").expect("revisions");

    // audit: [CandidateCreated@1, Approved@2, Edited@3]
    // revisions: CandidateCreated@seq=1, Edited@seq=3
    assert_eq!(audit.len(), 3);
    assert_eq!(revisions.len(), 2);
    assert_eq!(revisions[0].sequence, audit[0].sequence());
    assert_eq!(revisions[1].sequence, audit[2].sequence());
}

#[test]
fn revisions_returns_error_for_unknown_memory() {
    let store = InMemoryMemoryStore::default();
    let result = store.revisions("nonexistent");
    assert!(result.is_err());
}
