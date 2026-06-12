use std::sync::atomic::{AtomicU64, Ordering};

use codel00p_protocol::{
    MemoryAuditEntry, MemoryEntry, MemoryReviewAction, MemoryStatus, NewMemoryCandidate, Project,
    ProjectRef,
};
use codel00p_storage::{AppendLogStore, DocumentStore, StorageDocument, StorageScope};

use crate::error::ApiError;

const MEMORY_COLLECTION: &str = "memory";
static MEMORY_COUNTER: AtomicU64 = AtomicU64::new(1);

fn memory_scope(org_id: &str, project_id: &str) -> StorageScope {
    StorageScope::project(org_id, project_id)
}

fn audit_stream(memory_id: &str) -> String {
    format!("memory/{memory_id}")
}

/// Pushes a memory candidate into a project's review queue. The entry is always
/// created with `candidate` status; a `created` audit event is recorded.
pub fn push_candidate<S: DocumentStore + AppendLogStore + ?Sized>(
    store: &mut S,
    org_id: &str,
    project: &Project,
    request: NewMemoryCandidate,
    actor: &str,
) -> Result<MemoryEntry, ApiError> {
    let content = request.content.trim();
    if content.is_empty() {
        return Err(ApiError::BadRequest("memory content is required".into()));
    }

    let id = format!("mem_{}", MEMORY_COUNTER.fetch_add(1, Ordering::Relaxed));
    let mut entry = MemoryEntry::new(
        &id,
        ProjectRef::new(project.id(), project.name()),
        request.kind,
        content,
    )
    .with_sensitivity(request.sensitivity);
    for tag in request.tags {
        entry = entry.with_tag(tag);
    }

    let scope = memory_scope(org_id, project.id());
    let payload = serde_json::to_value(&entry).map_err(internal)?;
    let mut document = StorageDocument::new(scope.clone(), MEMORY_COLLECTION, &id, payload);
    // The cloud entry's source model is turn-based; retain any pushed origin URI
    // as document metadata until richer source links exist.
    if let Some(uri) = &request.source_uri {
        document = document.with_metadata("source_uri", uri);
    }
    store.put_document(document).map_err(internal)?;

    record_audit(store, &scope, &id, MemoryReviewAction::Created, actor)?;
    Ok(entry)
}

/// Lists a project's memory, optionally filtered to a single status.
pub fn list_memory<S: DocumentStore + ?Sized>(
    store: &S,
    org_id: &str,
    project_id: &str,
    status: Option<MemoryStatus>,
) -> Result<Vec<MemoryEntry>, ApiError> {
    let scope = memory_scope(org_id, project_id);
    let documents = store
        .list_documents(&scope, MEMORY_COLLECTION)
        .map_err(internal)?;

    documents
        .into_iter()
        .map(|document| {
            serde_json::from_value::<MemoryEntry>(document.payload().clone())
                .map_err(|err| ApiError::Internal(format!("corrupt memory record: {err}")))
        })
        .filter(|entry| match (status, entry) {
            (Some(wanted), Ok(entry)) => entry.status() == wanted,
            _ => true,
        })
        .collect()
}

/// Applies a review action to a memory entry, transitioning its status and
/// appending an audit event. Returns the updated entry.
pub fn review<S: DocumentStore + AppendLogStore + ?Sized>(
    store: &mut S,
    org_id: &str,
    project_id: &str,
    memory_id: &str,
    action: MemoryReviewAction,
    actor: &str,
) -> Result<MemoryEntry, ApiError> {
    let new_status = action
        .resulting_status()
        .ok_or_else(|| ApiError::Internal("review action has no resulting status".into()))?;
    let scope = memory_scope(org_id, project_id);

    let document = store
        .get_document(&scope, MEMORY_COLLECTION, memory_id)
        .map_err(internal)?
        .ok_or_else(|| ApiError::NotFound(format!("memory {memory_id} not found")))?;
    let entry: MemoryEntry = serde_json::from_value(document.payload().clone())
        .map_err(|err| ApiError::Internal(format!("corrupt memory record: {err}")))?;

    let updated = entry.with_status(new_status);
    let payload = serde_json::to_value(&updated).map_err(internal)?;
    let mut next = StorageDocument::new(scope.clone(), MEMORY_COLLECTION, memory_id, payload);
    for (key, value) in document.metadata() {
        next = next.with_metadata(key, value);
    }
    store.put_document(next).map_err(internal)?;

    record_audit(store, &scope, memory_id, action, actor)?;
    Ok(updated)
}

/// Returns the review audit trail for a memory entry, oldest first.
pub fn audit<S: AppendLogStore + ?Sized>(
    store: &S,
    org_id: &str,
    project_id: &str,
    memory_id: &str,
) -> Result<Vec<MemoryAuditEntry>, ApiError> {
    let scope = memory_scope(org_id, project_id);
    let entries = store
        .replay_log(&scope, &audit_stream(memory_id))
        .map_err(internal)?;

    entries
        .into_iter()
        .map(|entry| {
            serde_json::from_value::<MemoryAuditEntry>(entry.payload().clone())
                .map_err(|err| ApiError::Internal(format!("corrupt audit record: {err}")))
        })
        .collect()
}

/// RAG retrieval: returns approved memory most relevant to a free-text query,
/// ranked by keyword overlap — the grounding primitive agents use. An empty
/// query returns the most recent approved memory up to `limit`. (Embedding /
/// pgvector ranking will replace the keyword scorer.)
pub fn search<S: DocumentStore + ?Sized>(
    store: &S,
    org_id: &str,
    project_id: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<MemoryEntry>, ApiError> {
    let approved = list_memory(store, org_id, project_id, Some(MemoryStatus::Approved))?;
    let terms: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .map(str::to_string)
        .collect();

    if terms.is_empty() {
        let mut entries = approved;
        entries.truncate(limit);
        return Ok(entries);
    }

    let mut scored: Vec<(usize, MemoryEntry)> = approved
        .into_iter()
        .map(|entry| {
            let content = entry.content().to_lowercase();
            let score: usize = terms
                .iter()
                .map(|term| content.matches(term.as_str()).count())
                .sum();
            (score, entry)
        })
        .filter(|(score, _)| *score > 0)
        .collect();
    scored.sort_by(|left, right| right.0.cmp(&left.0));
    Ok(scored
        .into_iter()
        .take(limit)
        .map(|(_, entry)| entry)
        .collect())
}

fn record_audit<S: AppendLogStore + ?Sized>(
    store: &mut S,
    scope: &StorageScope,
    memory_id: &str,
    action: MemoryReviewAction,
    actor: &str,
) -> Result<(), ApiError> {
    let event = MemoryAuditEntry::new(memory_id, action, actor);
    let payload = serde_json::to_value(&event).map_err(internal)?;
    store
        .append_log_entry(scope.clone(), audit_stream(memory_id), payload)
        .map_err(internal)?;
    Ok(())
}

fn internal(error: impl std::fmt::Display) -> ApiError {
    ApiError::Internal(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codel00p_protocol::MemoryKind;
    use codel00p_storage::InMemoryStorage;

    fn project() -> Project {
        Project::new("proj_1", "org_acme", "codel00p", "codel00p")
    }

    #[test]
    fn candidate_pushes_then_reviews_with_audit_trail() {
        let mut store = InMemoryStorage::default();
        let project = project();

        let candidate = push_candidate(
            &mut store,
            "org_acme",
            &project,
            NewMemoryCandidate::new(MemoryKind::Convention, "Run cargo from core/."),
            "user_agent",
        )
        .expect("push");
        assert_eq!(candidate.status(), MemoryStatus::Candidate);

        // Visible as a candidate, not yet approved.
        assert_eq!(
            list_memory(&store, "org_acme", "proj_1", Some(MemoryStatus::Candidate))
                .expect("list")
                .len(),
            1
        );
        assert!(
            list_memory(&store, "org_acme", "proj_1", Some(MemoryStatus::Approved))
                .expect("list")
                .is_empty()
        );

        let approved = review(
            &mut store,
            "org_acme",
            "proj_1",
            candidate.id(),
            MemoryReviewAction::Approved,
            "user_admin",
        )
        .expect("approve");
        assert_eq!(approved.status(), MemoryStatus::Approved);

        // Now pullable as approved memory.
        let approved_list =
            list_memory(&store, "org_acme", "proj_1", Some(MemoryStatus::Approved)).expect("list");
        assert_eq!(approved_list.len(), 1);
        assert_eq!(approved_list[0].content(), "Run cargo from core/.");

        // Audit trail records both the creation and the approval.
        let trail = audit(&store, "org_acme", "proj_1", candidate.id()).expect("audit");
        let actions: Vec<MemoryReviewAction> = trail.iter().map(|entry| entry.action).collect();
        assert_eq!(
            actions,
            vec![MemoryReviewAction::Created, MemoryReviewAction::Approved]
        );
        assert_eq!(trail[1].actor, "user_admin");
    }

    #[test]
    fn search_ranks_approved_memory_by_keyword() {
        let mut store = InMemoryStorage::default();
        let project = project();

        let approve = |store: &mut InMemoryStorage, content: &str| {
            let entry = push_candidate(
                store,
                "org_acme",
                &project,
                NewMemoryCandidate::new(MemoryKind::Convention, content),
                "agent",
            )
            .expect("push");
            review(
                store,
                "org_acme",
                "proj_1",
                entry.id(),
                MemoryReviewAction::Approved,
                "admin",
            )
            .expect("approve");
        };

        approve(&mut store, "Run cargo from core/ for tests");
        approve(&mut store, "Deploy with the release script");
        // A candidate (unapproved) must never surface in RAG results.
        push_candidate(
            &mut store,
            "org_acme",
            &project,
            NewMemoryCandidate::new(MemoryKind::Decision, "cargo nightly only"),
            "agent",
        )
        .expect("push candidate");

        let hits = search(&store, "org_acme", "proj_1", "cargo tests", 10).expect("search");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].content().contains("cargo"));

        // Empty query returns approved memory (the corpus), not candidates.
        let all = search(&store, "org_acme", "proj_1", "  ", 10).expect("search empty");
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn memory_is_isolated_per_project() {
        let mut store = InMemoryStorage::default();
        push_candidate(
            &mut store,
            "org_acme",
            &Project::new("proj_1", "org_acme", "a", "a"),
            NewMemoryCandidate::new(MemoryKind::Decision, "one"),
            "u",
        )
        .expect("push");

        assert!(
            list_memory(&store, "org_acme", "proj_2", None)
                .expect("list")
                .is_empty()
        );
    }

    #[test]
    fn reviewing_missing_memory_is_not_found() {
        let mut store = InMemoryStorage::default();
        let error = review(
            &mut store,
            "org_acme",
            "proj_1",
            "mem_missing",
            MemoryReviewAction::Approved,
            "user_admin",
        )
        .unwrap_err();
        assert!(matches!(error, ApiError::NotFound(_)));
    }

    #[test]
    fn push_rejects_blank_content() {
        let mut store = InMemoryStorage::default();
        let error = push_candidate(
            &mut store,
            "org_acme",
            &project(),
            NewMemoryCandidate::new(MemoryKind::Workflow, "   "),
            "u",
        )
        .unwrap_err();
        assert!(matches!(error, ApiError::BadRequest(_)));
    }
}
