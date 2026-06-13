//! MCP tool handlers backed by codel00p memory and session stores.

use super::*;

pub(super) fn call_tool(config: &CliConfig, params: &Value) -> Result<McpServerResponse, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "tools/call omitted name".to_string())?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let (text, updated_resource_uris) = match name {
        "memory_similar" => (memory_similar(config, &arguments)?, Vec::new()),
        "memory_stale" => (memory_stale(config, &arguments)?, Vec::new()),
        "memory_quality" => (memory_quality(config, &arguments)?, Vec::new()),
        "memory_search" => (memory_search(config, &arguments)?, Vec::new()),
        "memory_list" => (memory_list(config, &arguments)?, Vec::new()),
        "memory_show" => (memory_show(config, &arguments)?, Vec::new()),
        "memory_audit" => (memory_audit(config, &arguments)?, Vec::new()),
        "memory_create_candidate" => memory_create_candidate(config, &arguments)?,
        "memory_approve" => memory_review(config, &arguments, MemoryReviewAction::Approve)?,
        "memory_reject" => memory_review(config, &arguments, MemoryReviewAction::Reject)?,
        "memory_archive" => memory_review(config, &arguments, MemoryReviewAction::Archive)?,
        "memory_edit" => memory_edit(config, &arguments)?,
        "memory_restore" => memory_restore(config, &arguments)?,
        "session_show" => (session_show(config, &arguments)?, Vec::new()),
        _ => return Err(format!("unknown codel00p MCP tool: {name}")),
    };
    let mut response = McpServerResponse::new(json!({
        "content": [
            { "type": "text", "text": text }
        ],
        "isError": false
    }));
    for uri in updated_resource_uris {
        response = response.with_updated_resource(uri);
    }
    Ok(response)
}

fn memory_quality(config: &CliConfig, arguments: &Value) -> Result<String, String> {
    let mut query = MemoryQualityQuery::new(config.project.clone());
    if let Some(status) = optional_string(arguments, "status") {
        query = query.with_status(parse_status(status)?);
    }
    if let Some(kind) = optional_string(arguments, "kind") {
        query = query.with_kind(parse_kind(kind)?);
    }
    if let Some(sensitivity) = optional_string(arguments, "sensitivity") {
        query = query.with_sensitivity(parse_sensitivity(sensitivity)?);
    }
    if let Some(tag) = optional_string(arguments, "tag") {
        query = query.with_tag(tag);
    }
    if let Some(max_score) = optional_usize(arguments, "max_score")? {
        if max_score > 100 {
            return Err("argument `max_score` must be between 0 and 100".to_string());
        }
        query = query.with_max_score(max_score as u8);
    }
    if let Some(limit) = optional_usize(arguments, "limit")? {
        query = query.with_limit(limit);
    }

    let store = open_memory_store(config)?;
    let records = store
        .quality_review(query)
        .map_err(|error| error.to_string())?;
    let items = records.iter().map(quality_memory_json).collect::<Vec<_>>();
    serde_json::to_string(&items).map_err(|error| error.to_string())
}

fn memory_similar(config: &CliConfig, arguments: &Value) -> Result<String, String> {
    let content = required_string(arguments, "content")?;
    let kind = parse_kind(required_string(arguments, "kind")?)?;
    let mut query = MemorySimilarityQuery::new(config.project.clone(), kind, content);
    if let Some(threshold) = optional_usize(arguments, "threshold")? {
        if threshold > 100 {
            return Err("argument `threshold` must be between 0 and 100".to_string());
        }
        query = query.with_min_score(threshold as u8);
    }
    if let Some(limit) = optional_usize(arguments, "limit")? {
        query = query.with_limit(limit);
    }

    let store = open_memory_store(config)?;
    let records = store
        .similar_active(query)
        .map_err(|error| error.to_string())?;
    let items = records.iter().map(similar_memory_json).collect::<Vec<_>>();
    serde_json::to_string(&items).map_err(|error| error.to_string())
}

fn memory_stale(config: &CliConfig, arguments: &Value) -> Result<String, String> {
    let mut query = MemoryStalenessQuery::new(config.project.clone());
    if let Some(kind) = optional_string(arguments, "kind") {
        query = query.with_kind(parse_kind(kind)?);
    }
    if let Some(threshold) = optional_usize(arguments, "threshold")? {
        if threshold > 100 {
            return Err("argument `threshold` must be between 0 and 100".to_string());
        }
        query = query.with_min_score(threshold as u8);
    }
    if let Some(limit) = optional_usize(arguments, "limit")? {
        query = query.with_limit(limit);
    }

    let store = open_memory_store(config)?;
    let records = store
        .stale_active(query)
        .map_err(|error| error.to_string())?;
    let items = records.iter().map(stale_memory_json).collect::<Vec<_>>();
    serde_json::to_string(&items).map_err(|error| error.to_string())
}

fn memory_search(config: &CliConfig, arguments: &Value) -> Result<String, String> {
    let mut query = MemoryQuery::new(config.project.clone());
    if let Some(text) = optional_string(arguments, "text") {
        query = query.with_text(text);
    }
    if let Some(kind) = optional_string(arguments, "kind") {
        query = query.with_kind(parse_kind(kind)?);
    }
    if let Some(sensitivity) = optional_string(arguments, "sensitivity") {
        query = query.with_sensitivity(parse_sensitivity(sensitivity)?);
    }
    if let Some(tag) = optional_string(arguments, "tag") {
        query = query.with_tag(tag);
    }
    if let Some(limit) = optional_usize(arguments, "limit")? {
        query = query.with_limit(limit);
    }

    let store = open_memory_store(config)?;
    let records = store.retrieve(query).map_err(|error| error.to_string())?;
    let items = records
        .iter()
        .map(retrieved_memory_json)
        .collect::<Vec<_>>();
    serde_json::to_string(&items).map_err(|error| error.to_string())
}

fn memory_list(config: &CliConfig, arguments: &Value) -> Result<String, String> {
    let mut filter = MemoryListFilter::new(config.project.clone());
    if let Some(status) = optional_string(arguments, "status") {
        filter = filter.with_status(parse_status(status)?);
    }
    if let Some(kind) = optional_string(arguments, "kind") {
        filter = filter.with_kind(parse_kind(kind)?);
    }
    if let Some(sensitivity) = optional_string(arguments, "sensitivity") {
        filter = filter.with_sensitivity(parse_sensitivity(sensitivity)?);
    }
    if let Some(tag) = optional_string(arguments, "tag") {
        filter = filter.with_tag(tag);
    }
    if let Some(limit) = optional_usize(arguments, "limit")? {
        filter = filter.with_limit(limit);
    }

    let store = open_memory_store(config)?;
    let records = store.list(filter).map_err(|error| error.to_string())?;
    let items = records.iter().map(memory_record_json).collect::<Vec<_>>();
    serde_json::to_string(&items).map_err(|error| error.to_string())
}

fn memory_show(config: &CliConfig, arguments: &Value) -> Result<String, String> {
    let id = required_string(arguments, "id")?;
    let store = open_memory_store(config)?;
    let record = store.get(id).map_err(|error| error.to_string())?;
    serde_json::to_string(&memory_record_json(&record)).map_err(|error| error.to_string())
}

fn memory_audit(config: &CliConfig, arguments: &Value) -> Result<String, String> {
    let id = required_string(arguments, "id")?;
    let store = open_memory_store(config)?;
    let events = store.audit_log(id).map_err(|error| error.to_string())?;
    let items = events
        .iter()
        .map(|event| {
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
            item
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&items).map_err(|error| error.to_string())
}

fn memory_create_candidate(
    config: &CliConfig,
    arguments: &Value,
) -> Result<(String, Vec<String>), String> {
    let id = required_string(arguments, "id")?;
    let mut source = MemorySource::turn(
        parse_session_id(required_string(arguments, "session_id")?)?,
        parse_turn_id(required_string(arguments, "turn_id")?)?,
    );
    if let Some(source_uri) = optional_string(arguments, "source_uri") {
        source = source.with_uri(source_uri);
    }
    let mut input = MemoryCandidateInput::new(
        id,
        config.project.clone(),
        parse_kind(required_string(arguments, "kind")?)?,
        required_string(arguments, "content")?,
        source,
    );
    for tag in optional_string_array(arguments, "tags")? {
        input = input.with_tag(tag);
    }
    if let Some(sensitivity) = optional_string(arguments, "sensitivity") {
        input = input.with_sensitivity(parse_sensitivity(sensitivity)?);
    }

    let mut store = open_memory_store(config)?;
    let record = store
        .create_candidate(input)
        .map_err(|error| error.to_string())?;
    let text =
        serde_json::to_string(&memory_record_json(&record)).map_err(|error| error.to_string())?;
    Ok((text, vec![memory_resource_uri(id)]))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MemoryReviewAction {
    Approve,
    Reject,
    Archive,
}

fn memory_review(
    config: &CliConfig,
    arguments: &Value,
    action: MemoryReviewAction,
) -> Result<(String, Vec<String>), String> {
    let id = required_string(arguments, "id")?;
    let actor = required_string(arguments, "actor")?;
    let decision = match action {
        MemoryReviewAction::Approve => ReviewDecision::approve(actor),
        MemoryReviewAction::Reject => ReviewDecision::reject(
            actor,
            required_string(arguments, "reason")
                .map_err(|_| "memory_reject requires reason".to_string())?,
        ),
        MemoryReviewAction::Archive => ReviewDecision::archive(
            actor,
            required_string(arguments, "reason")
                .map_err(|_| "memory_archive requires reason".to_string())?,
        ),
    };
    let mut store = open_memory_store(config)?;
    let record = store
        .review(id, decision)
        .map_err(|error| error.to_string())?;
    let text =
        serde_json::to_string(&memory_record_json(&record)).map_err(|error| error.to_string())?;
    Ok((text, vec![memory_resource_uri(id)]))
}

fn memory_edit(config: &CliConfig, arguments: &Value) -> Result<(String, Vec<String>), String> {
    let id = required_string(arguments, "id")?;
    let actor = required_string(arguments, "actor")?;
    let content = required_string(arguments, "content")?;
    let mut edit = MemoryEdit::replace_content(actor, content);
    if let Some(reason) = optional_string(arguments, "reason") {
        edit = edit.with_reason(reason);
    }

    let mut store = open_memory_store(config)?;
    let record = store.edit(id, edit).map_err(|error| error.to_string())?;
    let text =
        serde_json::to_string(&memory_record_json(&record)).map_err(|error| error.to_string())?;
    Ok((text, vec![memory_resource_uri(id)]))
}

fn memory_restore(config: &CliConfig, arguments: &Value) -> Result<(String, Vec<String>), String> {
    let id = required_string(arguments, "id")?;
    let sequence = required_u64(arguments, "sequence")?;
    let actor = required_string(arguments, "actor")?;
    let reason = optional_string(arguments, "reason")
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("restore audit sequence {sequence}"));

    let mut store = open_memory_store(config)?;
    let audit = store.audit_log(id).map_err(|error| error.to_string())?;
    let previous_content = audit
        .iter()
        .find(|event| event.sequence() == sequence)
        .ok_or_else(|| format!("memory audit sequence not found: {sequence}"))?
        .previous_content()
        .ok_or_else(|| format!("memory audit sequence {sequence} has no previous content"))?
        .to_string();

    let mut edit = MemoryEdit::replace_content(actor, previous_content);
    edit = edit.with_reason(reason);
    let record = store.edit(id, edit).map_err(|error| error.to_string())?;
    let text =
        serde_json::to_string(&memory_record_json(&record)).map_err(|error| error.to_string())?;
    Ok((text, vec![memory_resource_uri(id)]))
}

fn memory_resource_uri(id: &str) -> String {
    format!("codel00p://memory/{id}")
}

fn session_show(config: &CliConfig, arguments: &Value) -> Result<String, String> {
    let session_id = parse_session_id(required_string(arguments, "session_id")?)?;
    let store = open_session_store(config)?;
    let records = store
        .replay(&session_id)
        .map_err(|error| error.to_string())?;
    let items = session_records_json(&records);
    serde_json::to_string(&items).map_err(|error| error.to_string())
}
