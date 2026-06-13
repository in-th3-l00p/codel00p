use super::support::*;

#[test]
fn list_returns_all_review_states_for_project_in_deterministic_order() {
    let mut store = InMemoryMemoryStore::default();
    for id in [
        "mem-candidate",
        "mem-approved",
        "mem-rejected",
        "mem-archived",
    ] {
        store
            .create_candidate(MemoryCandidateInput::new(
                id,
                project(),
                MemoryKind::Workflow,
                format!("Lifecycle memory {id}."),
                source(),
            ))
            .expect("create candidate");
    }
    store
        .review("mem-approved", ReviewDecision::approve("alice"))
        .expect("approve memory");
    store
        .review("mem-rejected", ReviewDecision::reject("alice", "too vague"))
        .expect("reject memory");
    store
        .review("mem-archived", ReviewDecision::approve("alice"))
        .expect("approve memory");
    store
        .review("mem-archived", ReviewDecision::archive("alice", "obsolete"))
        .expect("archive memory");

    let listed = store
        .list(MemoryListFilter::new(project()))
        .expect("list memory");

    assert_eq!(
        listed
            .iter()
            .map(|record| (record.entry().id(), record.entry().status()))
            .collect::<Vec<_>>(),
        [
            ("mem-approved", MemoryStatus::Approved),
            ("mem-archived", MemoryStatus::Archived),
            ("mem-candidate", MemoryStatus::Candidate),
            ("mem-rejected", MemoryStatus::Rejected),
        ]
    );
}

#[test]
fn list_filters_by_status_kind_tag_and_limit() {
    let mut store = InMemoryMemoryStore::default();
    for (id, kind, tag) in [
        ("mem-a", MemoryKind::Workflow, "verify"),
        ("mem-b", MemoryKind::Architecture, "harness"),
        ("mem-c", MemoryKind::Workflow, "verify"),
    ] {
        store
            .create_candidate(
                MemoryCandidateInput::new(
                    id,
                    project(),
                    kind,
                    format!("Filtered memory {id}."),
                    source(),
                )
                .with_tag(tag),
            )
            .expect("create candidate");
        store
            .review(id, ReviewDecision::approve("alice"))
            .expect("approve memory");
    }

    let listed = store
        .list(
            MemoryListFilter::new(project())
                .with_status(MemoryStatus::Approved)
                .with_kind(MemoryKind::Workflow)
                .with_tag("verify")
                .with_limit(1),
        )
        .expect("list memory");

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].entry().id(), "mem-a");
}

#[test]
fn list_ignores_empty_optional_filters() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(
            MemoryCandidateInput::new(
                "mem-1",
                project(),
                MemoryKind::Workflow,
                "Run pnpm verify before pushing main.",
                source(),
            )
            .with_tag("verify"),
        )
        .expect("create candidate");

    let listed = store
        .list(MemoryListFilter::new(project()).with_tag(" ").with_limit(0))
        .expect("list memory");

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].entry().id(), "mem-1");
}
