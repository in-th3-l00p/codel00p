use super::support::*;

#[test]
fn creates_candidate_memory_with_required_source_and_tags() {
    let mut store = InMemoryMemoryStore::default();

    let candidate = store
        .create_candidate(
            MemoryCandidateInput::new(
                "mem-1",
                project(),
                MemoryKind::Architecture,
                "The harness owns tool execution.",
                source(),
            )
            .with_tag("harness")
            .with_tag("runtime"),
        )
        .expect("create candidate");

    assert_eq!(candidate.entry().id(), "mem-1");
    assert_eq!(candidate.entry().status(), MemoryStatus::Candidate);
    assert_eq!(candidate.entry().tags(), ["harness", "runtime"]);
}

#[test]
fn rejects_empty_candidate_content() {
    let mut store = InMemoryMemoryStore::default();

    let error = store
        .create_candidate(MemoryCandidateInput::new(
            "mem-empty",
            project(),
            MemoryKind::Workflow,
            " ",
            source(),
        ))
        .expect_err("empty content must fail");

    assert!(matches!(error, MemoryError::InvalidCandidate { .. }));
}

#[test]
fn rejects_exact_duplicate_active_memory_content() {
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

    let error = store
        .create_candidate(MemoryCandidateInput::new(
            "mem-duplicate",
            project(),
            MemoryKind::Workflow,
            " Run pnpm verify before pushing main. ",
            source(),
        ))
        .expect_err("duplicate content must fail");
    let listed = store
        .list(MemoryListFilter::new(project()))
        .expect("list memory");

    assert!(matches!(
        &error,
        MemoryError::DuplicateMemory { id, existing_id }
            if id == "mem-duplicate" && existing_id == "mem-original"
    ));
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].entry().id(), "mem-original");
}

#[test]
fn memory_quality_score_flags_low_value_content() {
    let mut store = InMemoryMemoryStore::default();
    let strong = store
        .create_candidate(MemoryCandidateInput::new(
            "mem-strong",
            project(),
            MemoryKind::Workflow,
            "Run pnpm verify before pushing main after editing provider policy.",
            source(),
        ))
        .expect("create strong candidate");
    let vague = store
        .create_candidate(MemoryCandidateInput::new(
            "mem-vague",
            project(),
            MemoryKind::Decision,
            "This is important.",
            source(),
        ))
        .expect("create vague candidate");

    assert_eq!(strong.quality().score(), 100);
    assert!(strong.quality().findings().is_empty());
    assert_eq!(vague.quality().score(), 65);
    assert_eq!(
        vague.quality().findings(),
        [
            "content is too short to be reusable",
            "content uses vague language"
        ]
    );
}

#[test]
fn quality_review_lists_low_quality_active_memory() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-vague",
            project(),
            MemoryKind::Decision,
            "This is important.",
            source(),
        ))
        .expect("create vague candidate");
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-strong",
            project(),
            MemoryKind::Workflow,
            "Run pnpm verify before pushing main after editing provider policy.",
            source(),
        ))
        .expect("create strong candidate");
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-rejected",
            project(),
            MemoryKind::Decision,
            "That thing matters.",
            source(),
        ))
        .expect("create rejected candidate");
    store
        .review("mem-rejected", ReviewDecision::reject("alice", "too vague"))
        .expect("reject low-quality candidate");

    let low_quality = store
        .quality_review(MemoryQualityQuery::new(project()).with_max_score(80))
        .expect("list low-quality memory");

    assert_eq!(low_quality.len(), 1);
    assert_eq!(low_quality[0].entry().id(), "mem-vague");
    assert_eq!(low_quality[0].quality().score(), 65);
    assert_eq!(
        low_quality[0].quality().findings(),
        [
            "content is too short to be reusable",
            "content uses vague language"
        ]
    );
}

#[test]
fn quality_review_can_filter_low_quality_memory_by_status() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-low-quality-candidate",
            project(),
            MemoryKind::Workflow,
            "Run tests.",
            source(),
        ))
        .expect("create low-quality candidate");
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-low-quality-approved",
            project(),
            MemoryKind::Workflow,
            "Use credential.",
            source(),
        ))
        .expect("create low-quality approved candidate");
    store
        .review("mem-low-quality-approved", ReviewDecision::approve("alice"))
        .expect("approve low-quality memory");

    let low_quality = store
        .quality_review(MemoryQualityQuery::new(project()).with_status(MemoryStatus::Approved))
        .expect("list low-quality approved memory");

    assert_eq!(low_quality.len(), 1);
    assert_eq!(low_quality[0].entry().id(), "mem-low-quality-approved");
    assert_eq!(low_quality[0].entry().status(), MemoryStatus::Approved);
}

#[test]
fn quality_review_can_filter_low_quality_memory_by_kind() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-vague-decision",
            project(),
            MemoryKind::Decision,
            "This is important.",
            source(),
        ))
        .expect("create vague decision candidate");
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-short-workflow",
            project(),
            MemoryKind::Workflow,
            "Run tests.",
            source(),
        ))
        .expect("create short workflow candidate");

    let low_quality = store
        .quality_review(
            MemoryQualityQuery::new(project())
                .with_kind(MemoryKind::Workflow)
                .with_max_score(80),
        )
        .expect("list low-quality workflow memory");

    assert_eq!(low_quality.len(), 1);
    assert_eq!(low_quality[0].entry().id(), "mem-short-workflow");
    assert_eq!(low_quality[0].entry().kind(), MemoryKind::Workflow);
}

#[test]
fn quality_review_can_filter_low_quality_memory_by_sensitivity() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-vague-normal",
            project(),
            MemoryKind::Workflow,
            "Run tests.",
            source(),
        ))
        .expect("create low-quality normal candidate");
    store
        .create_candidate(
            MemoryCandidateInput::new(
                "mem-vague-sensitive",
                project(),
                MemoryKind::Workflow,
                "Use credential.",
                source(),
            )
            .with_sensitivity(MemorySensitivity::Sensitive),
        )
        .expect("create low-quality sensitive candidate");

    let low_quality = store
        .quality_review(
            MemoryQualityQuery::new(project()).with_sensitivity(MemorySensitivity::Sensitive),
        )
        .expect("list low-quality sensitive memory");

    assert_eq!(low_quality.len(), 1);
    assert_eq!(low_quality[0].entry().id(), "mem-vague-sensitive");
    assert_eq!(
        low_quality[0].entry().sensitivity(),
        MemorySensitivity::Sensitive
    );
}

#[test]
fn quality_review_can_filter_low_quality_memory_by_tag() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(
            MemoryCandidateInput::new(
                "mem-vague-credential",
                project(),
                MemoryKind::Workflow,
                "Use credential.",
                source(),
            )
            .with_tag("credential"),
        )
        .expect("create low-quality credential candidate");
    store
        .create_candidate(
            MemoryCandidateInput::new(
                "mem-vague-verify",
                project(),
                MemoryKind::Workflow,
                "Run tests.",
                source(),
            )
            .with_tag("verify"),
        )
        .expect("create low-quality verify candidate");

    let low_quality = store
        .quality_review(MemoryQualityQuery::new(project()).with_tag("credential"))
        .expect("list low-quality memory by tag");

    assert_eq!(low_quality.len(), 1);
    assert_eq!(low_quality[0].entry().id(), "mem-vague-credential");
    assert_eq!(low_quality[0].entry().tags(), ["credential"]);
}
