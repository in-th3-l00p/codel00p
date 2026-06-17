use codel00p_memory::MemoryRevision;
use codel00p_protocol::{MemoryEvidence, MemorySource};
use serde_json::{Value, json};

use super::parse::{
    audit_action_label, evidence_kind_label, kind_label, sensitivity_label, status_label,
    visibility_label,
};

pub(super) fn memory_record_json(record: &codel00p_memory::MemoryRecord) -> Value {
    let mut item = memory_entry_json(record.entry());
    let quality = record.quality();
    item["quality"] = memory_quality_json(&quality);
    item
}

pub(super) fn retrieved_memory_json(memory: &codel00p_memory::RetrievedMemory) -> Value {
    let mut item = memory_entry_json(memory.entry());
    let quality = memory.quality();
    item["quality"] = memory_quality_json(&quality);
    item["reason"] = json!(memory.reason());
    item
}

pub(super) fn ranked_memory_json(memory: &codel00p_memory::RankedMemory) -> Value {
    let mut item = memory_entry_json(memory.entry());
    let quality = memory.quality();
    item["quality"] = memory_quality_json(&quality);
    item["score"] = json!(memory.score());
    item
}

pub(super) fn similar_memory_json(memory: &codel00p_memory::SimilarMemory) -> Value {
    let mut item = memory_entry_json(memory.entry());
    let quality = memory.quality();
    item["quality"] = memory_quality_json(&quality);
    item["score"] = json!(memory.score());
    item
}

pub(super) fn stale_memory_json(memory: &codel00p_memory::StaleMemory) -> Value {
    let mut item = memory_entry_json(memory.entry());
    let quality = memory.quality();
    let newer_quality = memory.newer_quality();
    item["quality"] = memory_quality_json(&quality);
    item["score"] = json!(memory.score());
    item["newer"] = memory_entry_json(memory.newer_entry());
    item["newer"]["quality"] = memory_quality_json(&newer_quality);
    item
}

pub(super) fn quality_memory_json(memory: &codel00p_memory::QualityMemory) -> Value {
    let mut item = memory_entry_json(memory.entry());
    item["quality"] = memory_quality_json(memory.quality());
    item
}

pub(super) fn memory_revision_json(rev: &MemoryRevision) -> Value {
    let mut item = json!({
        "revision": rev.revision,
        "sequence": rev.sequence,
        "action": super::parse::audit_action_label(rev.action),
        "actor": rev.actor,
        "content": rev.content,
    });
    if let Some(reason) = &rev.reason {
        item["reason"] = json!(reason);
    }
    item
}

pub(super) fn audit_event_json(event: &codel00p_memory::MemoryAuditEvent) -> Value {
    let mut item = json!({
        "memory_id": event.memory_id(),
        "sequence": event.sequence(),
        "action": audit_action_label(event.action()),
        "actor": event.actor(),
    });
    if let Some(reason) = event.reason() {
        item["reason"] = json!(reason);
    }
    if let Some(previous_content) = event.previous_content() {
        item["previous_content"] = json!(previous_content);
    }
    if let Some(new_content) = event.new_content() {
        item["new_content"] = json!(new_content);
    }
    if let Some(merged_into) = event.merged_into() {
        item["merged_into"] = json!(merged_into);
    }
    if let Some(split_into) = event.split_into() {
        item["split_into"] = json!(split_into);
    }
    if let Some(evidence_reference) = event.evidence_reference() {
        item["evidence_reference"] = json!(evidence_reference);
    }
    item
}

fn memory_entry_json(entry: &codel00p_protocol::MemoryEntry) -> Value {
    let mut item = json!({
        "id": entry.id(),
        "status": status_label(entry.status()),
        "kind": kind_label(entry.kind()),
        "sensitivity": sensitivity_label(entry.sensitivity()),
        "visibility": visibility_label(entry.visibility()),
        "content": entry.content(),
        "tags": entry.tags(),
    });
    if let Some(source) = entry.source() {
        let mut source_json = json!({
            "session_id": source.session_id().as_str(),
            "turn_id": source.turn_id().as_str(),
        });
        if let Some(uri) = source.uri() {
            source_json["uri"] = json!(uri);
        }
        item["source"] = source_json;
        item["source_uri"] = json!(source_uri(source));
    }
    if !entry.evidence().is_empty() {
        item["evidence"] = json!(
            entry
                .evidence()
                .iter()
                .map(evidence_json)
                .collect::<Vec<_>>()
        );
    }
    item
}

pub(super) fn evidence_json(evidence: &MemoryEvidence) -> Value {
    let mut item = json!({
        "kind": evidence_kind_label(evidence.kind()),
        "reference": evidence.reference(),
    });
    if let Some(note) = evidence.note() {
        item["note"] = json!(note);
    }
    item
}

fn memory_quality_json(quality: &codel00p_memory::MemoryQuality) -> Value {
    json!({
        "score": quality.score(),
        "findings": quality.findings(),
    })
}

pub(super) fn source_uri(source: &MemorySource) -> String {
    if let Some(uri) = source.uri() {
        return uri.to_string();
    }

    format!("codel00p://sessions/{}", source.session_id().as_str())
}
