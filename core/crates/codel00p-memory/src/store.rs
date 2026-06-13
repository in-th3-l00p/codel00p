//! Storage-backed implementation of the memory repository contract.

use codel00p_protocol::{MemoryEntry, MemoryKind, MemorySensitivity, MemoryStatus};
use codel00p_storage::{AppendLogStore, DocumentStore, StorageDocument};

use crate::{
    MemoryAuditAction, MemoryAuditEvent, MemoryCandidateInput, MemoryEdit, MemoryError,
    MemoryListFilter, MemoryQualityQuery, MemoryQuery, MemoryRecord, MemoryRepository,
    MemorySimilarityQuery, MemoryStalenessQuery, QualityMemory, RetrievedMemory, ReviewDecision,
    SimilarMemory, StaleMemory, StorageBackedMemoryStore,
    records::{content_tokens, token_similarity_score},
};

const MEMORY_COLLECTION: &str = "memory_entries";
const MEMORY_AUDIT_STREAM_PREFIX: &str = "memory";

impl<S> MemoryRepository for StorageBackedMemoryStore<S>
where
    S: DocumentStore + AppendLogStore,
{
    fn create_candidate(
        &mut self,
        input: MemoryCandidateInput,
    ) -> Result<MemoryRecord, MemoryError> {
        input.validate()?;
        if self
            .storage
            .get_document(&self.scope, MEMORY_COLLECTION, &input.id)?
            .is_some()
        {
            return Err(MemoryError::MemoryAlreadyExists { id: input.id });
        }

        let entry = input.into_entry();
        if let Some(existing_id) = self.active_duplicate_id(&entry)? {
            return Err(MemoryError::DuplicateMemory {
                id: entry.id().to_string(),
                existing_id,
            });
        }

        let record = MemoryRecord::new(entry);
        self.put_record(&record)?;
        self.append_audit(MemoryAuditEvent::candidate_created(record.entry().id()))?;
        self.append_index(MemoryAuditEvent::candidate_created(record.entry().id()))?;

        Ok(record)
    }

    fn review(&mut self, id: &str, decision: ReviewDecision) -> Result<MemoryRecord, MemoryError> {
        let current = self.get(id)?;
        ensure_transition(current.entry().status(), decision.next_status())?;
        let entry = set_status(current.entry().clone(), decision.next_status());
        let record = MemoryRecord::new(entry);

        self.put_record(&record)?;
        self.append_audit(MemoryAuditEvent::reviewed(id, &decision))?;

        Ok(record)
    }

    fn edit(&mut self, id: &str, edit: MemoryEdit) -> Result<MemoryRecord, MemoryError> {
        edit.validate()?;
        let current = self.get(id)?;
        let previous_content = current.entry().content().to_string();
        let new_content = edit.content().to_string();
        let entry = replace_content(current.entry(), new_content.clone());
        let record = MemoryRecord::new(entry);

        self.put_record(&record)?;
        self.append_audit(MemoryAuditEvent::edited(
            id,
            &edit,
            previous_content,
            new_content,
        ))?;

        Ok(record)
    }

    fn get(&self, id: &str) -> Result<MemoryRecord, MemoryError> {
        self.storage
            .get_document(&self.scope, MEMORY_COLLECTION, id)?
            .ok_or_else(|| MemoryError::MemoryNotFound { id: id.to_string() })
            .and_then(|document| Ok(serde_json::from_value(document.payload().clone())?))
    }

    fn audit_log(&self, id: &str) -> Result<Vec<MemoryAuditEvent>, MemoryError> {
        self.storage
            .replay_log(&self.scope, &memory_audit_stream(id))?
            .into_iter()
            .map(MemoryAuditEvent::from_log_entry)
            .collect()
    }

    fn list(&self, filter: MemoryListFilter) -> Result<Vec<MemoryRecord>, MemoryError> {
        let mut records = Vec::new();
        for record in self.records()? {
            if record.entry().project().id() != filter.project.id() {
                continue;
            }

            if let Some(status) = filter.status
                && record.entry().status() != status
            {
                continue;
            }

            if let Some(kind) = filter.kind
                && record.entry().kind() != kind
            {
                continue;
            }

            if let Some(sensitivity) = filter.sensitivity
                && record.entry().sensitivity() != sensitivity
            {
                continue;
            }

            if let Some(tag) = &filter.tag
                && !record
                    .entry()
                    .tags()
                    .iter()
                    .any(|candidate| candidate == tag)
            {
                continue;
            }

            records.push(record);
        }

        records.sort_by(|left, right| left.entry().id().cmp(right.entry().id()));
        if let Some(limit) = filter.limit {
            records.truncate(limit);
        }
        Ok(records)
    }

    fn retrieve(&self, query: MemoryQuery) -> Result<Vec<RetrievedMemory>, MemoryError> {
        let mut retrieved = Vec::new();
        for record in self.records()? {
            if record.entry().status() != MemoryStatus::Approved {
                continue;
            }

            if record.entry().project().id() != query.project.id() {
                continue;
            }

            if let Some(sensitivity) = query.sensitivity {
                if record.entry().sensitivity() != sensitivity {
                    continue;
                }
            } else if record.entry().sensitivity() == MemorySensitivity::Sensitive {
                continue;
            }

            let mut reasons = Vec::new();
            if let Some(kind) = query.kind {
                if record.entry().kind() != kind {
                    continue;
                }
                reasons.push(format!("kind {}", memory_kind_label(kind)));
            }

            if let Some(tag) = &query.tag {
                if !record
                    .entry()
                    .tags()
                    .iter()
                    .any(|candidate| candidate == tag)
                {
                    continue;
                }
                reasons.push(format!("tag {tag}"));
            }

            if let Some(text) = &query.text {
                if !entry_content(record.entry())
                    .to_lowercase()
                    .contains(&text.to_lowercase())
                {
                    continue;
                }
                reasons.push(format!("text {text}"));
            }

            if let Some(sensitivity) = query.sensitivity {
                reasons.push(format!(
                    "sensitivity {}",
                    memory_sensitivity_label(sensitivity)
                ));
            }

            let reason = if reasons.is_empty() {
                "matched approved project memory".to_string()
            } else {
                format!("matched {}", reasons.join(" and "))
            };

            retrieved.push(RetrievedMemory { record, reason });
        }

        retrieved.sort_by(|left, right| left.entry().id().cmp(right.entry().id()));
        if let Some(limit) = query.limit {
            retrieved.truncate(limit);
        }
        Ok(retrieved)
    }

    fn similar_active(
        &self,
        query: MemorySimilarityQuery,
    ) -> Result<Vec<SimilarMemory>, MemoryError> {
        let query_tokens = content_tokens(&query.content);
        let mut similar = Vec::new();

        for record in self.records()? {
            let entry = record.entry();
            if !is_active_memory_status(entry.status()) {
                continue;
            }

            if entry.project().id() != query.project.id() || entry.kind() != query.kind {
                continue;
            }

            let score = token_similarity_score(&query_tokens, &content_tokens(entry.content()));
            if score < query.min_score {
                continue;
            }

            similar.push(SimilarMemory { record, score });
        }

        similar.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.entry().id().cmp(right.entry().id()))
        });
        if let Some(limit) = query.limit {
            similar.truncate(limit);
        }
        Ok(similar)
    }

    fn stale_active(&self, query: MemoryStalenessQuery) -> Result<Vec<StaleMemory>, MemoryError> {
        let records = self.indexed_records()?;
        let mut stale = Vec::new();

        for older in &records {
            let older_entry = older.record.entry();
            if older_entry.status() != MemoryStatus::Approved {
                continue;
            }

            if older_entry.project().id() != query.project.id() {
                continue;
            }

            if let Some(kind) = query.kind
                && older_entry.kind() != kind
            {
                continue;
            }

            let older_tokens = content_tokens(older_entry.content());
            let mut best_newer: Option<StaleMemory> = None;

            for newer in records
                .iter()
                .filter(|newer| newer.sequence > older.sequence)
            {
                let newer_entry = newer.record.entry();
                if !is_active_memory_status(newer_entry.status()) {
                    continue;
                }

                if newer_entry.project().id() != older_entry.project().id()
                    || newer_entry.kind() != older_entry.kind()
                {
                    continue;
                }

                let score =
                    token_similarity_score(&older_tokens, &content_tokens(newer_entry.content()));
                if score < query.min_score {
                    continue;
                }

                let candidate = StaleMemory {
                    record: older.record.clone(),
                    newer_record: newer.record.clone(),
                    score,
                };
                let replace_best = best_newer.as_ref().is_none_or(|best| {
                    candidate.score > best.score
                        || (candidate.score == best.score
                            && candidate.newer_entry().id() < best.newer_entry().id())
                });
                if replace_best {
                    best_newer = Some(candidate);
                }
            }

            if let Some(best_newer) = best_newer {
                stale.push(best_newer);
            }
        }

        stale.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.entry().id().cmp(right.entry().id()))
                .then_with(|| left.newer_entry().id().cmp(right.newer_entry().id()))
        });
        if let Some(limit) = query.limit {
            stale.truncate(limit);
        }
        Ok(stale)
    }

    fn quality_review(&self, query: MemoryQualityQuery) -> Result<Vec<QualityMemory>, MemoryError> {
        let mut low_quality = Vec::new();

        for record in self.records()? {
            let entry = record.entry();
            if !is_active_memory_status(entry.status()) {
                continue;
            }

            if entry.project().id() != query.project.id() {
                continue;
            }

            if query.status.is_some_and(|status| entry.status() != status) {
                continue;
            }

            if query.kind.is_some_and(|kind| entry.kind() != kind) {
                continue;
            }

            if query
                .sensitivity
                .is_some_and(|sensitivity| entry.sensitivity() != sensitivity)
            {
                continue;
            }

            if query
                .tag
                .as_ref()
                .is_some_and(|tag| !entry.tags().iter().any(|entry_tag| entry_tag == tag))
            {
                continue;
            }

            let quality = record.quality();
            if quality.score() > query.max_score {
                continue;
            }

            low_quality.push(QualityMemory { record, quality });
        }

        low_quality.sort_by(|left, right| {
            left.quality
                .score()
                .cmp(&right.quality.score())
                .then_with(|| left.entry().id().cmp(right.entry().id()))
        });
        if let Some(limit) = query.limit {
            low_quality.truncate(limit);
        }
        Ok(low_quality)
    }
}

impl<S> StorageBackedMemoryStore<S>
where
    S: DocumentStore + AppendLogStore,
{
    fn put_record(&mut self, record: &MemoryRecord) -> Result<(), MemoryError> {
        self.storage.put_document(StorageDocument::new(
            self.scope.clone(),
            MEMORY_COLLECTION,
            record.entry().id(),
            serde_json::to_value(record)?,
        ))?;
        Ok(())
    }

    fn append_audit(&mut self, event: MemoryAuditEvent) -> Result<(), MemoryError> {
        self.storage.append_log(
            self.scope.clone(),
            memory_audit_stream(&event.memory_id),
            serde_json::to_value(event)?,
        )?;
        Ok(())
    }

    fn append_index(&mut self, event: MemoryAuditEvent) -> Result<(), MemoryError> {
        self.storage.append_log(
            self.scope.clone(),
            MEMORY_AUDIT_STREAM_PREFIX,
            serde_json::to_value(event)?,
        )?;
        Ok(())
    }

    fn records(&self) -> Result<Vec<MemoryRecord>, MemoryError> {
        Ok(self
            .indexed_records()?
            .into_iter()
            .map(|record| record.record)
            .collect())
    }

    fn indexed_records(&self) -> Result<Vec<IndexedMemoryRecord>, MemoryError> {
        // The first storage contract has point reads and append logs. Keep a
        // deterministic index as an append stream until queryable collections
        // are added to codel00p-storage.
        let mut records = Vec::new();
        for audit in self
            .storage
            .replay_log(&self.scope, MEMORY_AUDIT_STREAM_PREFIX)?
            .into_iter()
            .map(MemoryAuditEvent::from_log_entry)
        {
            let audit = audit?;
            if audit.action() == MemoryAuditAction::CandidateCreated {
                records.push(IndexedMemoryRecord {
                    sequence: audit.sequence(),
                    record: self.get(audit.memory_id())?,
                });
            }
        }
        Ok(records)
    }

    fn active_duplicate_id(&self, entry: &MemoryEntry) -> Result<Option<String>, MemoryError> {
        for record in self.records()? {
            let existing = record.entry();
            if is_active_memory_status(existing.status())
                && existing.project().id() == entry.project().id()
                && existing.kind() == entry.kind()
                && existing.content().trim() == entry.content().trim()
            {
                return Ok(Some(existing.id().to_string()));
            }
        }
        Ok(None)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct IndexedMemoryRecord {
    record: MemoryRecord,
    sequence: u64,
}

fn ensure_transition(from: MemoryStatus, to: MemoryStatus) -> Result<(), MemoryError> {
    let allowed = matches!(
        (from, to),
        (MemoryStatus::Candidate, MemoryStatus::Approved)
            | (MemoryStatus::Candidate, MemoryStatus::Rejected)
            | (MemoryStatus::Approved, MemoryStatus::Archived)
    );

    if allowed {
        Ok(())
    } else {
        Err(MemoryError::InvalidTransition { from, to })
    }
}

fn is_active_memory_status(status: MemoryStatus) -> bool {
    matches!(status, MemoryStatus::Candidate | MemoryStatus::Approved)
}

fn set_status(entry: MemoryEntry, status: MemoryStatus) -> MemoryEntry {
    entry.with_status(status)
}

fn replace_content(entry: &MemoryEntry, content: String) -> MemoryEntry {
    let mut updated = MemoryEntry::new(
        entry.id().to_string(),
        entry.project().clone(),
        entry.kind(),
        content,
    )
    .with_status(entry.status())
    .with_sensitivity(entry.sensitivity());
    if let Some(source) = entry.source() {
        updated = updated.with_source(source.clone());
    }
    for tag in entry.tags() {
        updated = updated.with_tag(tag.clone());
    }
    updated
}

fn entry_content(entry: &MemoryEntry) -> &str {
    entry.content()
}

fn memory_kind_label(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Architecture => "architecture",
        MemoryKind::Convention => "convention",
        MemoryKind::Workflow => "workflow",
        MemoryKind::Decision => "decision",
        MemoryKind::Deployment => "deployment",
        MemoryKind::Troubleshooting => "troubleshooting",
    }
}

fn memory_sensitivity_label(sensitivity: MemorySensitivity) -> &'static str {
    match sensitivity {
        MemorySensitivity::Normal => "normal",
        MemorySensitivity::Sensitive => "sensitive",
    }
}

fn memory_audit_stream(id: &str) -> String {
    format!("{MEMORY_AUDIT_STREAM_PREFIX}/{id}")
}
