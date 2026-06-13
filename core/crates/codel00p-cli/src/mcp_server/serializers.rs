//! JSON serialization helpers for MCP memory and session payloads.

use super::*;

pub(super) fn session_records_json(
    records: &[codel00p_session::PersistedSessionRecord],
) -> Vec<Value> {
    records
        .iter()
        .map(|record| match record.record() {
            SessionRecord::Message(message) => json!({
                "sequence": record.sequence(),
                "type": "message",
                "role": session_role_label(message.role()),
                "summary": session_message_summary(message),
            }),
            SessionRecord::Event(event) => json!({
                "sequence": record.sequence(),
                "type": "event",
                "event": format!("{event:?}"),
            }),
        })
        .collect()
}

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

fn memory_entry_json(entry: &codel00p_protocol::MemoryEntry) -> Value {
    let mut item = json!({
        "id": entry.id(),
        "status": status_label(entry.status()),
        "kind": kind_label(entry.kind()),
        "sensitivity": sensitivity_label(entry.sensitivity()),
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
    item
}

fn memory_quality_json(quality: &codel00p_memory::MemoryQuality) -> Value {
    json!({
        "score": quality.score(),
        "findings": quality.findings(),
    })
}

fn source_uri(source: &MemorySource) -> String {
    if let Some(uri) = source.uri() {
        return uri.to_string();
    }

    format!("codel00p://sessions/{}", source.session_id().as_str())
}

fn status_label(status: MemoryStatus) -> &'static str {
    match status {
        MemoryStatus::Candidate => "candidate",
        MemoryStatus::Approved => "approved",
        MemoryStatus::Rejected => "rejected",
        MemoryStatus::Archived => "archived",
    }
}

fn sensitivity_label(sensitivity: MemorySensitivity) -> &'static str {
    match sensitivity {
        MemorySensitivity::Normal => "normal",
        MemorySensitivity::Sensitive => "sensitive",
    }
}

pub(super) fn audit_action_label(action: MemoryAuditAction) -> &'static str {
    match action {
        MemoryAuditAction::CandidateCreated => "candidate_created",
        MemoryAuditAction::Approved => "approved",
        MemoryAuditAction::Rejected => "rejected",
        MemoryAuditAction::Archived => "archived",
        MemoryAuditAction::Edited => "edited",
    }
}

fn kind_label(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Architecture => "architecture",
        MemoryKind::Convention => "convention",
        MemoryKind::Workflow => "workflow",
        MemoryKind::Decision => "decision",
        MemoryKind::Deployment => "deployment",
        MemoryKind::Troubleshooting => "troubleshooting",
    }
}

fn session_role_label(role: SessionRole) -> &'static str {
    match role {
        SessionRole::System => "system",
        SessionRole::User => "user",
        SessionRole::Assistant => "assistant",
        SessionRole::Tool => "tool",
    }
}

fn session_message_summary(message: &SessionMessage) -> String {
    if !message.content().is_empty() {
        return message.content().to_string();
    }
    if !message.tool_calls().is_empty() {
        return format!("{} tool call(s)", message.tool_calls().len());
    }
    String::new()
}
