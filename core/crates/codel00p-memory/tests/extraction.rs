use codel00p_memory::{
    ExplicitMemoryExtractor, InMemoryMemoryStore, MemoryCandidateExtractor, MemoryExtractionInput,
    MemoryRepository, ReviewDecision,
};
use codel00p_protocol::{MemoryKind, MemorySource, MemoryStatus, ProjectRef, SessionId, TurnId};

fn project() -> ProjectRef {
    ProjectRef::new("project-1", "codel00p")
}

fn source() -> MemorySource {
    MemorySource::turn(
        SessionId::from_static("session-1"),
        TurnId::from_static("turn-1"),
    )
}

#[test]
fn explicit_extractor_turns_remember_directives_into_candidates() {
    let extractor = ExplicitMemoryExtractor;

    let candidates = extractor
        .extract(
            MemoryExtractionInput::new(
                project(),
                source(),
                "\
Finished the harness integration.
remember architecture[harness,runtime]: The harness owns tool execution.
remember workflow[verify]: Run pnpm verify before pushing main.
",
            )
            .with_tag("turn-summary"),
        )
        .expect("extract candidates");

    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].id(), "memory-candidate-session-1-turn-1-1");
    assert_eq!(candidates[0].kind(), MemoryKind::Architecture);
    assert_eq!(candidates[0].content(), "The harness owns tool execution.");
    assert_eq!(candidates[0].tags(), ["turn-summary", "harness", "runtime"]);
    assert_eq!(candidates[1].id(), "memory-candidate-session-1-turn-1-2");
    assert_eq!(candidates[1].kind(), MemoryKind::Workflow);
    assert_eq!(candidates[1].tags(), ["turn-summary", "verify"]);
}

#[test]
fn explicit_extractor_defaults_kind_and_normalizes_whitespace() {
    let extractor = ExplicitMemoryExtractor;

    let candidates = extractor
        .extract(MemoryExtractionInput::new(
            project(),
            source(),
            "  remember:   The team prefers small commits.   ",
        ))
        .expect("extract candidates");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].kind(), MemoryKind::Decision);
    assert_eq!(candidates[0].content(), "The team prefers small commits.");
}

#[test]
fn explicit_extractor_ignores_unknown_kinds_and_empty_content() {
    let extractor = ExplicitMemoryExtractor;

    let candidates = extractor
        .extract(MemoryExtractionInput::new(
            project(),
            source(),
            "\
remember unknown[harness]: Should not be extracted.
remember workflow:
ordinary prose should not become memory
",
        ))
        .expect("extract candidates");

    assert!(candidates.is_empty());
}

#[test]
fn explicit_extractor_keeps_candidate_ids_deterministic_after_ignored_lines() {
    let extractor = ExplicitMemoryExtractor;

    let candidates = extractor
        .extract(MemoryExtractionInput::new(
            project(),
            source(),
            "\
remember nope: ignored
remember deployment[cloud]: Cloud memory sync should only publish approved memories.
",
        ))
        .expect("extract candidates");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id(), "memory-candidate-session-1-turn-1-1");
    assert_eq!(candidates[0].kind(), MemoryKind::Deployment);
}

#[test]
fn extracted_candidates_enter_the_review_lifecycle() {
    let extractor = ExplicitMemoryExtractor;
    let mut store = InMemoryMemoryStore::default();
    let candidates = extractor
        .extract(MemoryExtractionInput::new(
            project(),
            source(),
            "remember convention[commits]: Keep commits small and focused.",
        ))
        .expect("extract candidates");

    for candidate in candidates {
        store
            .create_candidate(candidate)
            .expect("persist extracted candidate");
    }

    let candidate = store
        .get("memory-candidate-session-1-turn-1-1")
        .expect("load candidate");
    let approved = store
        .review(
            "memory-candidate-session-1-turn-1-1",
            ReviewDecision::approve("alice"),
        )
        .expect("approve candidate");

    assert_eq!(candidate.entry().status(), MemoryStatus::Candidate);
    assert_eq!(candidate.entry().kind(), MemoryKind::Convention);
    assert_eq!(approved.entry().status(), MemoryStatus::Approved);
}
