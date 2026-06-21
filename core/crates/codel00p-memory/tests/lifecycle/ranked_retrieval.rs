use super::support::*;

fn approved(store: &mut InMemoryMemoryStore, id: &str, kind: MemoryKind, content: &str, tag: &str) {
    store
        .create_candidate(
            MemoryCandidateInput::new(id, project(), kind, content, source()).with_tag(tag),
        )
        .expect("create candidate");
    store
        .review(id, ReviewDecision::approve("alice"))
        .expect("approve candidate");
}

#[test]
fn ranked_retrieval_orders_more_similar_content_first() {
    let mut store = InMemoryMemoryStore::default();
    approved(
        &mut store,
        "mem-close",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing to main branch.",
        "verify",
    );
    approved(
        &mut store,
        "mem-partial",
        MemoryKind::Workflow,
        "Run pnpm verify after editing provider policy.",
        "verify",
    );
    approved(
        &mut store,
        "mem-far",
        MemoryKind::Workflow,
        "The harness owns tool execution.",
        "harness",
    );

    let ranked = store
        .retrieve_ranked(
            MemoryRetrievalQuery::new(project(), "Run pnpm verify before pushing main branch.")
                .with_min_score(1),
        )
        .expect("retrieve ranked memory");

    let ids = ranked
        .iter()
        .map(|memory| memory.entry().id())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["mem-close", "mem-partial"]);
    assert!(ranked[0].score() > ranked[1].score());
}

#[test]
fn ranked_retrieval_breaks_ties_by_memory_id() {
    let mut store = InMemoryMemoryStore::default();
    // Distinct content (so duplicate detection allows them) that share the same
    // token set, so each scores identically against the query and the only
    // deterministic ordering signal is the memory id.
    for (id, content) in [
        ("mem-c", "Run pnpm verify before pushing main."),
        ("mem-a", "Main pushing before verify pnpm run."),
        ("mem-b", "Verify pnpm run before pushing main."),
    ] {
        approved(&mut store, id, MemoryKind::Workflow, content, "verify");
    }

    let ranked = store
        .retrieve_ranked(MemoryRetrievalQuery::new(
            project(),
            "Run pnpm verify before pushing main.",
        ))
        .expect("retrieve ranked memory");

    let ids = ranked
        .iter()
        .map(|memory| memory.entry().id())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["mem-a", "mem-b", "mem-c"]);
    // Identical token sets score identically; the tie-break is purely by id.
    assert!(
        ranked
            .iter()
            .all(|memory| memory.score() == ranked[0].score())
    );
}

#[test]
fn ranked_retrieval_bm25_weighs_rare_terms_above_common_ones() {
    // "deploy" appears in every candidate (common ⇒ low idf); "kubernetes"
    // appears in only one (rare ⇒ high idf). A query mentioning both must rank
    // the rare-term match first, which plain token-overlap Jaccard would not
    // guarantee. This exercises BM25 idf through the repository path.
    let mut store = InMemoryMemoryStore::default();
    approved(
        &mut store,
        "mem-rare",
        MemoryKind::Deployment,
        "Deploy the service to the kubernetes cluster.",
        "deploy",
    );
    approved(
        &mut store,
        "mem-common-a",
        MemoryKind::Deployment,
        "Deploy the service to the staging environment first.",
        "deploy",
    );
    approved(
        &mut store,
        "mem-common-b",
        MemoryKind::Deployment,
        "Deploy the service after the release notes are ready.",
        "deploy",
    );

    let ranked = store
        .retrieve_ranked(MemoryRetrievalQuery::new(project(), "deploy to kubernetes"))
        .expect("retrieve ranked memory");

    assert_eq!(ranked[0].entry().id(), "mem-rare");
    assert!(
        ranked[0].score() > ranked[1].score(),
        "rare-term match {} should outrank common-only match {}",
        ranked[0].score(),
        ranked[1].score()
    );
}

#[test]
fn ranked_retrieval_applies_kind_filter_before_ranking() {
    let mut store = InMemoryMemoryStore::default();
    approved(
        &mut store,
        "mem-workflow",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main branch.",
        "verify",
    );
    approved(
        &mut store,
        "mem-architecture",
        MemoryKind::Architecture,
        "Run pnpm verify before pushing main branch.",
        "verify",
    );

    let ranked = store
        .retrieve_ranked(
            MemoryRetrievalQuery::new(project(), "Run pnpm verify before pushing main branch.")
                .with_kind(MemoryKind::Workflow),
        )
        .expect("retrieve ranked memory");

    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].entry().id(), "mem-workflow");
}

#[test]
fn ranked_retrieval_applies_tag_filter_before_ranking() {
    let mut store = InMemoryMemoryStore::default();
    approved(
        &mut store,
        "mem-verify",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main branch.",
        "verify",
    );
    approved(
        &mut store,
        "mem-other",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing the release branch.",
        "release",
    );

    let ranked = store
        .retrieve_ranked(
            MemoryRetrievalQuery::new(project(), "Run pnpm verify before pushing main branch.")
                .with_tag("verify"),
        )
        .expect("retrieve ranked memory");

    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].entry().id(), "mem-verify");
}

#[test]
fn ranked_retrieval_respects_limit() {
    let mut store = InMemoryMemoryStore::default();
    for (id, content) in [
        ("mem-c", "Run pnpm verify before pushing main."),
        ("mem-a", "Main pushing before verify pnpm run."),
        ("mem-b", "Verify pnpm run before pushing main."),
    ] {
        approved(&mut store, id, MemoryKind::Workflow, content, "verify");
    }

    let ranked = store
        .retrieve_ranked(
            MemoryRetrievalQuery::new(project(), "Run pnpm verify before pushing main.")
                .with_limit(2),
        )
        .expect("retrieve ranked memory");

    let ids = ranked
        .iter()
        .map(|memory| memory.entry().id())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["mem-a", "mem-b"]);
}

#[test]
fn ranked_retrieval_excludes_sensitive_unless_requested() {
    let mut store = InMemoryMemoryStore::default();
    approved(
        &mut store,
        "mem-normal",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
    );
    store
        .create_candidate(
            MemoryCandidateInput::new(
                "mem-sensitive",
                project(),
                MemoryKind::Workflow,
                "Run pnpm verify before pushing main with the private credential.",
                source(),
            )
            .with_tag("verify")
            .with_sensitivity(MemorySensitivity::Sensitive),
        )
        .expect("create sensitive candidate");
    store
        .review("mem-sensitive", ReviewDecision::approve("alice"))
        .expect("approve sensitive memory");

    let default_ranked = store
        .retrieve_ranked(MemoryRetrievalQuery::new(
            project(),
            "Run pnpm verify before pushing main.",
        ))
        .expect("retrieve default ranked memory");
    let sensitive_ranked = store
        .retrieve_ranked(
            MemoryRetrievalQuery::new(project(), "Run pnpm verify before pushing main.")
                .with_sensitivity(MemorySensitivity::Sensitive),
        )
        .expect("retrieve sensitive ranked memory");

    let default_ids = default_ranked
        .iter()
        .map(|memory| memory.entry().id())
        .collect::<Vec<_>>();
    assert_eq!(default_ids, ["mem-normal"]);

    let sensitive_ids = sensitive_ranked
        .iter()
        .map(|memory| memory.entry().id())
        .collect::<Vec<_>>();
    assert_eq!(sensitive_ids, ["mem-sensitive"]);
}

#[test]
fn ranked_retrieval_returns_only_approved_memory() {
    let mut store = InMemoryMemoryStore::default();
    approved(
        &mut store,
        "mem-approved",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
    );
    // Candidate (unreviewed) memory must never be retrievable.
    store
        .create_candidate(
            MemoryCandidateInput::new(
                "mem-candidate",
                project(),
                MemoryKind::Workflow,
                "Run pnpm verify before pushing main always.",
                source(),
            )
            .with_tag("verify"),
        )
        .expect("create candidate");

    let ranked = store
        .retrieve_ranked(MemoryRetrievalQuery::new(
            project(),
            "Run pnpm verify before pushing main.",
        ))
        .expect("retrieve ranked memory");

    let ids = ranked
        .iter()
        .map(|memory| memory.entry().id())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["mem-approved"]);
}

#[test]
fn ranked_retrieval_drops_unrelated_content_by_default_threshold() {
    let mut store = InMemoryMemoryStore::default();
    approved(
        &mut store,
        "mem-match",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main branch.",
        "verify",
    );
    approved(
        &mut store,
        "mem-unrelated",
        MemoryKind::Workflow,
        "Configure the colorful unicorn dashboard widget.",
        "ui",
    );

    let ranked = store
        .retrieve_ranked(MemoryRetrievalQuery::new(
            project(),
            "Run pnpm verify before pushing main branch.",
        ))
        .expect("retrieve ranked memory");

    let ids = ranked
        .iter()
        .map(|memory| memory.entry().id())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["mem-match"]);
}
