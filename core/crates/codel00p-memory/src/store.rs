//! Storage-backed implementation of the memory repository contract.

use codel00p_protocol::{
    MemoryEntry, MemoryEvidence, MemoryKind, MemorySensitivity, MemorySource, MemoryStatus,
    MemoryVisibility, SessionId, TurnId,
};
use codel00p_storage::{AppendLogStore, DocumentStore, StorageDocument};

use crate::{
    MemoryAuditAction, MemoryAuditEvent, MemoryCandidateInput, MemoryEdit, MemoryError,
    MemoryListFilter, MemoryMerge, MemoryQualityQuery, MemoryQuery, MemoryRecord, MemoryRepository,
    MemoryRetrievalQuery, MemoryRevision, MemorySimilarityQuery, MemorySplit, MemoryStalenessQuery,
    QualityMemory, RankedMemory, RetrievedMemory, ReviewDecision, SimilarMemory, StaleMemory,
    StorageBackedMemoryStore,
    ranking::{RankingDocument, shingle_similarity},
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
        self.append_audit(MemoryAuditEvent::candidate_created(
            record.entry().id(),
            record.entry().content(),
        ))?;
        self.append_index(MemoryAuditEvent::candidate_created(
            record.entry().id(),
            record.entry().content(),
        ))?;

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

    fn add_evidence(
        &mut self,
        id: &str,
        evidence: MemoryEvidence,
        actor: &str,
        reason: Option<String>,
    ) -> Result<MemoryRecord, MemoryError> {
        let current = self.get(id)?;
        if !is_active_memory_status(current.entry().status()) {
            return Err(MemoryError::InvalidEdit {
                message: "cannot add evidence to a memory that is not active".to_string(),
            });
        }

        let reference = evidence.reference().to_string();
        let entry = append_evidence(current.entry(), evidence);
        let record = MemoryRecord::new(entry);

        self.put_record(&record)?;
        self.append_audit(MemoryAuditEvent::evidence_added(
            id, actor, reference, reason,
        ))?;

        Ok(record)
    }

    fn merge(
        &mut self,
        source_id: &str,
        target_id: &str,
        merge: MemoryMerge,
    ) -> Result<MemoryRecord, MemoryError> {
        if source_id == target_id {
            return Err(MemoryError::InvalidMerge {
                message: "cannot merge a memory into itself".to_string(),
            });
        }

        let source = self.get(source_id)?;
        let target = self.get(target_id)?;

        if source.entry().project().id() != target.entry().project().id() {
            return Err(MemoryError::InvalidMerge {
                message: "memories belong to different projects".to_string(),
            });
        }
        if !is_active_memory_status(source.entry().status()) {
            return Err(MemoryError::InvalidMerge {
                message: "source memory is not active".to_string(),
            });
        }
        if !is_active_memory_status(target.entry().status()) {
            return Err(MemoryError::InvalidMerge {
                message: "target memory is not active".to_string(),
            });
        }

        // Carry the duplicate's tags onto the survivor, then archive the source.
        let target_entry = union_tags(target.entry(), source.entry().tags());
        let target_record = MemoryRecord::new(target_entry);
        self.put_record(&target_record)?;

        let archived_source = set_status(source.entry().clone(), MemoryStatus::Archived);
        self.put_record(&MemoryRecord::new(archived_source))?;

        self.append_audit(MemoryAuditEvent::merged(source_id, &merge, target_id))?;
        self.append_audit(MemoryAuditEvent::merged_from(target_id, &merge, source_id))?;

        Ok(target_record)
    }

    fn split(&mut self, source_id: &str, split: MemorySplit) -> Result<MemoryRecord, MemoryError> {
        let source = self.get(source_id)?;

        if !is_active_memory_status(source.entry().status()) {
            return Err(MemoryError::InvalidSplit {
                message: "source memory is not active".to_string(),
            });
        }

        // Reject if new_id is already taken.
        if self
            .storage
            .get_document(&self.scope, MEMORY_COLLECTION, split.new_id())?
            .is_some()
        {
            return Err(MemoryError::MemoryAlreadyExists {
                id: split.new_id().to_string(),
            });
        }

        // Build a source reference pointing back to the source memory's session
        // source, so the new candidate retains provenance lineage.
        let new_source = source.entry().source().cloned().unwrap_or_else(|| {
            MemorySource::turn(
                SessionId::from_static("system"),
                TurnId::from_static("split"),
            )
        });

        // Create the new candidate inheriting project/kind/sensitivity/tags.
        let mut new_input = MemoryCandidateInput::new(
            split.new_id(),
            source.entry().project().clone(),
            source.entry().kind(),
            split.new_content(),
            new_source,
        )
        .with_sensitivity(source.entry().sensitivity())
        .with_visibility(source.entry().visibility());
        for tag in source.entry().tags() {
            new_input = new_input.with_tag(tag.clone());
        }

        let new_record = MemoryRecord::new(new_input.into_entry());
        self.put_record(&new_record)?;
        self.append_audit(MemoryAuditEvent::candidate_created(
            split.new_id(),
            new_record.entry().content(),
        ))?;
        self.append_index(MemoryAuditEvent::candidate_created(
            split.new_id(),
            new_record.entry().content(),
        ))?;

        // Optionally update source content.
        if let Some(updated_content) = split.updated_source_content() {
            let previous_content = source.entry().content().to_string();
            let source_entry = replace_content(source.entry(), updated_content.to_string());
            let updated_source = MemoryRecord::new(source_entry);
            self.put_record(&updated_source)?;
            // Record an edit event for the content update on the source.
            let edit = crate::MemoryEdit::replace_content(split.actor(), updated_content);
            self.append_audit(MemoryAuditEvent::edited(
                source_id,
                &edit,
                previous_content,
                updated_content,
            ))?;
        }

        // Two-sided split audit trail.
        self.append_audit(MemoryAuditEvent::split_source(source_id, &split))?;
        self.append_audit(MemoryAuditEvent::split_from(
            split.new_id(),
            &split,
            source_id,
        ))?;

        Ok(new_record)
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

    fn revisions(&self, id: &str) -> Result<Vec<MemoryRevision>, MemoryError> {
        // Verify the memory exists before walking its audit stream.
        self.get(id)?;

        let events = self.audit_log(id)?;
        let mut revision_number: u64 = 0;
        let mut revisions = Vec::new();

        for event in events {
            let content = match event.action() {
                MemoryAuditAction::CandidateCreated => event.new_content().map(str::to_owned),
                MemoryAuditAction::Edited => event.new_content().map(str::to_owned),
                _ => None,
            };

            if let Some(content) = content {
                revision_number += 1;
                revisions.push(MemoryRevision {
                    revision: revision_number,
                    sequence: event.sequence(),
                    actor: event.actor().to_string(),
                    action: event.action(),
                    content,
                    reason: event.reason().map(str::to_owned),
                });
            }
        }

        Ok(revisions)
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

            if let Some(visibility) = filter.visibility
                && record.entry().visibility() > visibility
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

            if let Some(visibility) = query.visibility
                && record.entry().visibility() > visibility
            {
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

            if let Some(visibility) = query.visibility {
                reasons.push(format!(
                    "visibility {}",
                    memory_visibility_label(visibility)
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

    fn retrieve_ranked(
        &self,
        query: MemoryRetrievalQuery,
    ) -> Result<Vec<RankedMemory>, MemoryError> {
        // Deterministic filters first, so the candidate set is bounded and
        // reproducible before any ranking happens. The surviving records form the
        // corpus over which BM25 computes inverse-document-frequency.
        let mut filtered = Vec::new();
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

            if let Some(visibility) = query.visibility
                && record.entry().visibility() > visibility
            {
                continue;
            }

            if let Some(kind) = query.kind
                && record.entry().kind() != kind
            {
                continue;
            }

            if let Some(tag) = &query.tag
                && !record
                    .entry()
                    .tags()
                    .iter()
                    .any(|candidate| candidate == tag)
            {
                continue;
            }

            filtered.push(record);
        }

        // Rank the filtered corpus through the injected provider. Scores live on
        // a 0..=100 scale: a near-perfect match lands near 100 and a candidate
        // sharing no query term scores 0, so the `min_score` threshold keeps its
        // prior meaning (default 1 ⇒ at least one query term must hit). The
        // default provider is offline BM25 (no embeddings, no network — see
        // `ranking::Bm25RankingProvider`); a host can inject an external ranker
        // via `with_ranker`. Either way the scoring contract is the same.
        let documents: Vec<RankingDocument> = filtered
            .iter()
            .map(|record| RankingDocument::new(record.entry().id(), record.entry().content()))
            .collect();
        let scores = self.ranker.rank(&query.query, &documents)?;
        if scores.len() != filtered.len() {
            return Err(MemoryError::Ranking {
                message: format!(
                    "ranking provider returned {} scores for {} documents",
                    scores.len(),
                    filtered.len()
                ),
            });
        }

        let mut ranked: Vec<RankedMemory> = filtered
            .iter()
            .zip(scores)
            .filter(|(_, score)| *score >= query.min_score)
            .map(|(record, score)| RankedMemory {
                record: record.clone(),
                score,
            })
            .collect();

        ranked.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.entry().id().cmp(right.entry().id()))
        });
        if let Some(limit) = query.limit {
            ranked.truncate(limit);
        }
        Ok(ranked)
    }

    fn similar_active(
        &self,
        query: MemorySimilarityQuery,
    ) -> Result<Vec<SimilarMemory>, MemoryError> {
        let mut similar = Vec::new();

        for record in self.records()? {
            let entry = record.entry();
            if !is_active_memory_status(entry.status()) {
                continue;
            }

            if entry.project().id() != query.project.id() || entry.kind() != query.kind {
                continue;
            }

            // Shingle (token n-gram) Jaccard catches reworded near-duplicates that
            // bag-of-words/substring matching misses, strengthening merge-candidate
            // detection. The human review gate is unchanged — this only ranks.
            let score = shingle_similarity(&query.content, entry.content());
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

                let score = shingle_similarity(older_entry.content(), newer_entry.content());
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
    .with_sensitivity(entry.sensitivity())
    .with_visibility(entry.visibility());
    if let Some(source) = entry.source() {
        updated = updated.with_source(source.clone());
    }
    for tag in entry.tags() {
        updated = updated.with_tag(tag.clone());
    }
    for evidence in entry.evidence() {
        updated = updated.with_evidence(evidence.clone());
    }
    updated
}

/// Rebuilds `entry` with one additional evidence link appended, preserving all
/// other fields including existing evidence.
fn append_evidence(entry: &MemoryEntry, evidence: MemoryEvidence) -> MemoryEntry {
    let mut updated = MemoryEntry::new(
        entry.id().to_string(),
        entry.project().clone(),
        entry.kind(),
        entry.content().to_string(),
    )
    .with_status(entry.status())
    .with_sensitivity(entry.sensitivity())
    .with_visibility(entry.visibility());
    if let Some(source) = entry.source() {
        updated = updated.with_source(source.clone());
    }
    for tag in entry.tags() {
        updated = updated.with_tag(tag.clone());
    }
    for existing in entry.evidence() {
        updated = updated.with_evidence(existing.clone());
    }
    updated.with_evidence(evidence)
}

/// Rebuilds `target` with the union of its own tags and `extra_tags`, preserving
/// the target's tag order and appending only tags it does not already carry.
fn union_tags(target: &MemoryEntry, extra_tags: &[String]) -> MemoryEntry {
    let mut entry = MemoryEntry::new(
        target.id().to_string(),
        target.project().clone(),
        target.kind(),
        target.content().to_string(),
    )
    .with_status(target.status())
    .with_sensitivity(target.sensitivity())
    .with_visibility(target.visibility());
    if let Some(source) = target.source() {
        entry = entry.with_source(source.clone());
    }
    for evidence in target.evidence() {
        entry = entry.with_evidence(evidence.clone());
    }

    let mut tags: Vec<String> = target.tags().to_vec();
    for tag in extra_tags {
        if !tags.iter().any(|existing| existing == tag) {
            tags.push(tag.clone());
        }
    }
    for tag in tags {
        entry = entry.with_tag(tag);
    }
    entry
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

fn memory_visibility_label(visibility: MemoryVisibility) -> &'static str {
    match visibility {
        MemoryVisibility::Private => "private",
        MemoryVisibility::Project => "project",
        MemoryVisibility::Team => "team",
        MemoryVisibility::Org => "org",
    }
}

fn memory_audit_stream(id: &str) -> String {
    format!("{MEMORY_AUDIT_STREAM_PREFIX}/{id}")
}
