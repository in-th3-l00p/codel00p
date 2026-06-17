use super::support::*;

#[test]
fn creates_candidate_with_explicit_evidence_links() {
    let mut store = InMemoryMemoryStore::default();

    let candidate = store
        .create_candidate(
            MemoryCandidateInput::new(
                "mem-1",
                project(),
                MemoryKind::Decision,
                "Adopted axum for the cloud service.",
                source(),
            )
            .with_evidence(
                MemoryEvidence::new(EvidenceKind::Pr, "https://github.com/acme/repo/pull/12")
                    .with_note("decision PR"),
            )
            .with_evidence(MemoryEvidence::new(
                EvidenceKind::File,
                "core/crates/codel00p-cloud/src/lib.rs",
            )),
        )
        .expect("create candidate with evidence");

    let evidence = candidate.entry().evidence();
    assert_eq!(evidence.len(), 2);
    assert_eq!(evidence[0].kind(), EvidenceKind::Pr);
    assert_eq!(
        evidence[0].reference(),
        "https://github.com/acme/repo/pull/12"
    );
    assert_eq!(evidence[0].note(), Some("decision PR"));
    assert_eq!(evidence[1].kind(), EvidenceKind::File);
    assert_eq!(evidence[1].note(), None);

    // Evidence survives a round-trip through storage.
    let reloaded = store.get("mem-1").expect("reload memory");
    assert_eq!(reloaded.entry().evidence().len(), 2);
}

#[test]
fn add_evidence_appends_to_existing_memory_and_audits_event() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-1",
            project(),
            MemoryKind::Troubleshooting,
            "Cargo tests must run serially due to a timing-sensitive MCP test.",
            source(),
        ))
        .expect("create candidate");
    store
        .review("mem-1", ReviewDecision::approve("alice"))
        .expect("approve candidate");

    let updated = store
        .add_evidence(
            "mem-1",
            MemoryEvidence::new(EvidenceKind::Commit, "deadbeef").with_note("fix commit"),
            "bob",
            Some("link the fix".to_string()),
        )
        .expect("add evidence");

    let evidence = updated.entry().evidence();
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].kind(), EvidenceKind::Commit);
    assert_eq!(evidence[0].reference(), "deadbeef");
    assert_eq!(evidence[0].note(), Some("fix commit"));
    // Status is preserved.
    assert_eq!(updated.entry().status(), MemoryStatus::Approved);

    // Evidence persisted to storage.
    let reloaded = store.get("mem-1").expect("reload memory");
    assert_eq!(reloaded.entry().evidence().len(), 1);

    let audit = store.audit_log("mem-1").expect("audit log");
    assert_eq!(audit.len(), 3);
    assert_eq!(audit[2].sequence(), 3);
    assert_eq!(audit[2].action(), MemoryAuditAction::EvidenceAdded);
    assert_eq!(audit[2].actor(), "bob");
    assert_eq!(audit[2].reason(), Some("link the fix"));
    assert_eq!(audit[2].evidence_reference(), Some("deadbeef"));

    let event_json = serde_json::to_value(&audit[2]).expect("serialize audit event");
    assert_eq!(event_json["action"], "evidence_added");
    assert_eq!(event_json["evidence_reference"], "deadbeef");
}

#[test]
fn add_evidence_appends_without_dropping_prior_links() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(
            MemoryCandidateInput::new(
                "mem-1",
                project(),
                MemoryKind::Decision,
                "Adopted axum for the cloud service.",
                source(),
            )
            .with_evidence(MemoryEvidence::new(
                EvidenceKind::Url,
                "https://example.com/rfc",
            )),
        )
        .expect("create candidate");

    let updated = store
        .add_evidence(
            "mem-1",
            MemoryEvidence::new(EvidenceKind::Issue, "ACME-42"),
            "carol",
            None,
        )
        .expect("add evidence");

    let evidence = updated.entry().evidence();
    assert_eq!(evidence.len(), 2);
    assert_eq!(evidence[0].kind(), EvidenceKind::Url);
    assert_eq!(evidence[1].kind(), EvidenceKind::Issue);
    assert_eq!(evidence[1].reference(), "ACME-42");
}

#[test]
fn add_evidence_to_archived_memory_fails() {
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-1",
            project(),
            MemoryKind::Decision,
            "Adopted axum for the cloud service.",
            source(),
        ))
        .expect("create candidate");
    store
        .review("mem-1", ReviewDecision::reject("alice", "superseded"))
        .expect("reject candidate");

    let error = store
        .add_evidence(
            "mem-1",
            MemoryEvidence::new(EvidenceKind::Other, "ref"),
            "bob",
            None,
        )
        .expect_err("evidence on inactive memory must fail");
    assert!(matches!(error, MemoryError::InvalidEdit { .. }));
}
