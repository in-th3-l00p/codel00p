use codel00p_memory::{
    InMemoryMemoryStore, MemoryAuditAction, MemoryCandidateInput, MemoryError, MemoryListFilter,
    MemoryQuery, MemoryRepository, ReviewDecision, StorageBackedMemoryStore,
};
use codel00p_protocol::{MemoryKind, MemorySource, MemoryStatus, ProjectRef, SessionId, TurnId};
use codel00p_storage::{InMemoryStorage, StorageScope};

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
fn retrieval_returns_only_approved_project_memory_with_reasons() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(
            MemoryCandidateInput::new(
                "mem-harness",
                project(),
                MemoryKind::Architecture,
                "The harness owns tool execution.",
                source(),
            )
            .with_tag("harness"),
        )
        .expect("create harness candidate");
    store
        .create_candidate(
            MemoryCandidateInput::new(
                "mem-provider",
                project(),
                MemoryKind::Architecture,
                "Providers own inference routing.",
                source(),
            )
            .with_tag("providers"),
        )
        .expect("create provider candidate");
    store
        .review("mem-harness", ReviewDecision::approve("alice"))
        .expect("approve harness memory");

    let retrieved = store
        .retrieve(
            MemoryQuery::new(project())
                .with_tag("harness")
                .with_text("tool execution"),
        )
        .expect("retrieve memory");

    assert_eq!(retrieved.len(), 1);
    assert_eq!(retrieved[0].entry().id(), "mem-harness");
    assert_eq!(
        retrieved[0].reason(),
        "matched tag harness and text tool execution"
    );
}

#[test]
fn memory_store_can_be_reopened_over_the_same_storage_backend() {
    let storage = InMemoryStorage::default();
    let scope = StorageScope::project("org-1", "project-1");
    let mut first = StorageBackedMemoryStore::new(scope.clone(), storage);

    first
        .create_candidate(MemoryCandidateInput::new(
            "mem-1",
            project(),
            MemoryKind::Workflow,
            "Run pnpm verify before pushing main.",
            source(),
        ))
        .expect("create candidate");
    first
        .review("mem-1", ReviewDecision::approve("alice"))
        .expect("approve candidate");

    let storage = first.into_inner();
    let second = StorageBackedMemoryStore::new(scope, storage);
    let loaded = second.get("mem-1").expect("load memory");
    let audit = second.audit_log("mem-1").expect("load audit");

    assert_eq!(loaded.entry().status(), MemoryStatus::Approved);
    assert_eq!(audit.len(), 2);
    assert_eq!(audit[1].action(), MemoryAuditAction::Approved);
}

#[test]
fn retrieval_is_deterministic_by_memory_id() {
    let mut store = InMemoryMemoryStore::default();
    for id in ["mem-c", "mem-a", "mem-b"] {
        store
            .create_candidate(
                MemoryCandidateInput::new(
                    id,
                    project(),
                    MemoryKind::Workflow,
                    format!("Verification workflow note {id}."),
                    source(),
                )
                .with_tag("verify"),
            )
            .expect("create candidate");
        store
            .review(id, ReviewDecision::approve("alice"))
            .expect("approve candidate");
    }

    let retrieved = store
        .retrieve(MemoryQuery::new(project()).with_tag("verify"))
        .expect("retrieve memory");

    assert_eq!(
        retrieved
            .iter()
            .map(|memory| memory.entry().id())
            .collect::<Vec<_>>(),
        ["mem-a", "mem-b", "mem-c"]
    );
}

#[test]
fn retrieval_can_filter_by_memory_kind() {
    let mut store = InMemoryMemoryStore::default();
    for (id, kind, content) in [
        (
            "mem-architecture",
            MemoryKind::Architecture,
            "The harness owns tool execution.",
        ),
        (
            "mem-workflow",
            MemoryKind::Workflow,
            "Run pnpm verify before pushing main.",
        ),
    ] {
        store
            .create_candidate(MemoryCandidateInput::new(
                id,
                project(),
                kind,
                content,
                source(),
            ))
            .expect("create candidate");
        store
            .review(id, ReviewDecision::approve("alice"))
            .expect("approve candidate");
    }

    let retrieved = store
        .retrieve(MemoryQuery::new(project()).with_kind(MemoryKind::Workflow))
        .expect("retrieve workflow memory");

    assert_eq!(retrieved.len(), 1);
    assert_eq!(retrieved[0].entry().id(), "mem-workflow");
    assert_eq!(retrieved[0].reason(), "matched kind workflow");
}

#[test]
fn retrieval_limit_caps_deterministic_results() {
    let mut store = InMemoryMemoryStore::default();
    for id in ["mem-c", "mem-a", "mem-b"] {
        store
            .create_candidate(MemoryCandidateInput::new(
                id,
                project(),
                MemoryKind::Workflow,
                format!("Verification workflow note {id}."),
                source(),
            ))
            .expect("create candidate");
        store
            .review(id, ReviewDecision::approve("alice"))
            .expect("approve candidate");
    }

    let retrieved = store
        .retrieve(MemoryQuery::new(project()).with_limit(2))
        .expect("retrieve capped memory");

    assert_eq!(
        retrieved
            .iter()
            .map(|memory| memory.entry().id())
            .collect::<Vec<_>>(),
        ["mem-a", "mem-b"]
    );
}

#[test]
fn retrieval_ignores_empty_optional_filters() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(
            MemoryCandidateInput::new(
                "mem-1",
                project(),
                MemoryKind::Architecture,
                "The harness owns tool execution.",
                source(),
            )
            .with_tag("harness"),
        )
        .expect("create candidate");
    store
        .review("mem-1", ReviewDecision::approve("alice"))
        .expect("approve candidate");

    let retrieved = store
        .retrieve(
            MemoryQuery::new(project())
                .with_tag(" ")
                .with_text(" ")
                .with_limit(0),
        )
        .expect("retrieve memory");

    assert_eq!(retrieved.len(), 1);
    assert_eq!(retrieved[0].reason(), "matched approved project memory");
}

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
