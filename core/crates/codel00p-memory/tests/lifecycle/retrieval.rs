use super::support::*;

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
fn retrieval_excludes_sensitive_memory_unless_explicitly_queried() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-normal",
            project(),
            MemoryKind::Workflow,
            "Run pnpm verify before pushing main.",
            source(),
        ))
        .expect("create normal candidate");
    store
        .review("mem-normal", ReviewDecision::approve("alice"))
        .expect("approve normal memory");
    store
        .create_candidate(
            MemoryCandidateInput::new(
                "mem-sensitive",
                project(),
                MemoryKind::Workflow,
                "Use the private deployment credential only from CI.",
                source(),
            )
            .with_sensitivity(MemorySensitivity::Sensitive),
        )
        .expect("create sensitive candidate");
    store
        .review("mem-sensitive", ReviewDecision::approve("alice"))
        .expect("approve sensitive memory");

    let default_retrieved = store
        .retrieve(MemoryQuery::new(project()))
        .expect("retrieve default memory");
    let sensitive_retrieved = store
        .retrieve(MemoryQuery::new(project()).with_sensitivity(MemorySensitivity::Sensitive))
        .expect("retrieve sensitive memory");

    assert_eq!(default_retrieved.len(), 1);
    assert_eq!(default_retrieved[0].entry().id(), "mem-normal");
    assert_eq!(sensitive_retrieved.len(), 1);
    assert_eq!(sensitive_retrieved[0].entry().id(), "mem-sensitive");
    assert_eq!(
        sensitive_retrieved[0].reason(),
        "matched sensitivity sensitive"
    );
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
