use super::support::*;

#[test]
fn finds_near_duplicate_active_memory() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-original",
            project(),
            MemoryKind::Workflow,
            "Run pnpm verify before pushing main.",
            source(),
        ))
        .expect("create original candidate");
    store
        .review("mem-original", ReviewDecision::approve("alice"))
        .expect("approve original");
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-unrelated",
            project(),
            MemoryKind::Workflow,
            "The harness owns tool execution.",
            source(),
        ))
        .expect("create unrelated candidate");
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-archived",
            project(),
            MemoryKind::Workflow,
            "Run pnpm verify before pushing main branch.",
            source(),
        ))
        .expect("create archived candidate");
    store
        .review("mem-archived", ReviewDecision::approve("alice"))
        .expect("approve archived candidate");
    store
        .review(
            "mem-archived",
            ReviewDecision::archive("alice", "superseded"),
        )
        .expect("archive similar memory");

    let similar = store
        .similar_active(
            MemorySimilarityQuery::new(
                project(),
                MemoryKind::Workflow,
                "Run pnpm verify before pushing to main branch.",
            )
            .with_min_score(70),
        )
        .expect("find similar memory");

    assert_eq!(similar.len(), 1);
    assert_eq!(similar[0].entry().id(), "mem-original");
    assert_eq!(similar[0].entry().status(), MemoryStatus::Approved);
    assert_eq!(similar[0].score(), 75);
}

#[test]
fn detects_stale_approved_memory_superseded_by_newer_active_memory() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-original",
            project(),
            MemoryKind::Workflow,
            "Run pnpm verify before pushing main.",
            source(),
        ))
        .expect("create original candidate");
    store
        .review("mem-original", ReviewDecision::approve("alice"))
        .expect("approve original");
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-unrelated",
            project(),
            MemoryKind::Workflow,
            "The harness owns tool execution.",
            source(),
        ))
        .expect("create unrelated candidate");
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-archived",
            project(),
            MemoryKind::Workflow,
            "Run pnpm verify before pushing to main branch.",
            source(),
        ))
        .expect("create archived candidate");
    store
        .review("mem-archived", ReviewDecision::approve("alice"))
        .expect("approve archived candidate");
    store
        .review(
            "mem-archived",
            ReviewDecision::archive("alice", "superseded"),
        )
        .expect("archive newer similar memory");
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-newer",
            project(),
            MemoryKind::Workflow,
            "Run pnpm verify before pushing to main branch.",
            source(),
        ))
        .expect("create newer active candidate");

    let stale = store
        .stale_active(
            MemoryStalenessQuery::new(project())
                .with_kind(MemoryKind::Workflow)
                .with_min_score(70),
        )
        .expect("detect stale memory");

    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].entry().id(), "mem-original");
    assert_eq!(stale[0].newer_entry().id(), "mem-newer");
    assert_eq!(stale[0].score(), 75);
}
