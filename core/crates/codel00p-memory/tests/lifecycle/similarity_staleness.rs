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
    // Similarity now uses token-bigram (shingle) Jaccard rather than unigram
    // Jaccard, so a reworded near-duplicate scores higher (83 vs the old 75):
    // shared phrasing ("run pnpm", "pnpm verify", "verify before", "before
    // pushing") is rewarded, which is exactly the merge-candidate signal we want.
    assert_eq!(similar[0].score(), 83);
}

#[test]
fn shingle_similarity_catches_reworded_near_duplicate() {
    // Two memories with the same meaning but reworded. Their unigram bag-of-words
    // overlap is modest (different connective/order words), but they share whole
    // bigrams ("cargo fmt", "cargo clippy", "before committing"), so shingle
    // Jaccard surfaces them as a strong merge candidate.
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-a",
            project(),
            MemoryKind::Convention,
            "Always run cargo fmt and cargo clippy before committing changes.",
            source(),
        ))
        .expect("create first");
    store
        .review("mem-a", ReviewDecision::approve("alice"))
        .expect("approve first");

    let similar = store
        .similar_active(
            MemorySimilarityQuery::new(
                project(),
                MemoryKind::Convention,
                "Before committing changes, always run cargo clippy and cargo fmt.",
            )
            .with_min_score(60),
        )
        .expect("find similar memory");

    assert_eq!(similar.len(), 1, "reworded duplicate should be detected");
    assert_eq!(similar[0].entry().id(), "mem-a");
    assert!(
        similar[0].score() >= 60,
        "reworded near-duplicate scored only {}",
        similar[0].score()
    );
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
    // Shingle (token-bigram) Jaccard scores the reworded supersession at 83,
    // up from the old unigram-Jaccard 75 — see `finds_near_duplicate_active_memory`.
    assert_eq!(stale[0].score(), 83);
}
