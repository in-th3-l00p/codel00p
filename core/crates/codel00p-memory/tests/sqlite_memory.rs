#![cfg(feature = "sqlite")]

use std::time::{SystemTime, UNIX_EPOCH};

use codel00p_memory::{
    ExplicitMemoryExtractor, MemoryCandidateExtractor, MemoryExtractionInput, MemoryListFilter,
    MemoryQuery, MemoryRepository, ReviewDecision, StorageBackedMemoryStore,
};
use codel00p_protocol::{MemoryKind, MemorySource, MemoryStatus, ProjectRef, SessionId, TurnId};
use codel00p_storage::{SqliteStorage, StorageScope};

fn temp_sqlite_path(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "codel00p-memory-{name}-{}-{nanos}.sqlite",
        std::process::id()
    ))
}

fn project() -> ProjectRef {
    ProjectRef::new("project-1", "codel00p")
}

fn source() -> MemorySource {
    MemorySource::turn(
        SessionId::from_static("session-sqlite"),
        TurnId::from_static("turn-sqlite"),
    )
}

#[test]
fn sqlite_memory_store_persists_extracted_candidates_reviews_and_retrieval() {
    let path = temp_sqlite_path("lifecycle");
    let scope = StorageScope::project("org-1", "project-1");

    {
        let storage = SqliteStorage::open(&path).expect("open sqlite storage");
        let mut store = StorageBackedMemoryStore::new(scope.clone(), storage);
        let candidates = ExplicitMemoryExtractor
            .extract(MemoryExtractionInput::new(
                project(),
                source(),
                "remember workflow[verify]: Run pnpm verify before pushing main.",
            ))
            .expect("extract candidates");

        for candidate in candidates {
            store.create_candidate(candidate).expect("create candidate");
        }
        store
            .review(
                "memory-candidate-session-sqlite-turn-sqlite-1",
                ReviewDecision::approve("alice"),
            )
            .expect("approve candidate");
    }

    let storage = SqliteStorage::open(&path).expect("reopen sqlite storage");
    let store = StorageBackedMemoryStore::new(scope, storage);
    let record = store
        .get("memory-candidate-session-sqlite-turn-sqlite-1")
        .expect("load record");
    let audit = store
        .audit_log("memory-candidate-session-sqlite-turn-sqlite-1")
        .expect("load audit log");
    let retrieved = store
        .retrieve(
            MemoryQuery::new(project())
                .with_kind(MemoryKind::Workflow)
                .with_tag("verify"),
        )
        .expect("retrieve approved memory");

    assert_eq!(record.entry().status(), MemoryStatus::Approved);
    assert_eq!(audit.len(), 2);
    assert_eq!(retrieved.len(), 1);
    assert_eq!(
        retrieved[0].reason(),
        "matched kind workflow and tag verify"
    );
    let listed = store
        .list(
            MemoryListFilter::new(project())
                .with_status(MemoryStatus::Approved)
                .with_kind(MemoryKind::Workflow)
                .with_tag("verify"),
        )
        .expect("list approved memory");
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0].entry().id(),
        "memory-candidate-session-sqlite-turn-sqlite-1"
    );

    let _ = std::fs::remove_file(path);
}
