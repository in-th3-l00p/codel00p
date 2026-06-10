use codel00p_memory::{
    MemoryEdit, MemoryListFilter, MemoryQuery, MemoryRepository, MemorySimilarityQuery,
    ReviewDecision,
};
use codel00p_protocol::{MemoryKind, MemoryStatus};
use serde_json::{Value, json};

use crate::config::{CliConfig, CliResult, open_memory_store, required_value};

pub fn run(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("missing memory command".to_string());
    };

    match command.as_str() {
        "search" => memory_search(config, rest),
        "similar" => memory_similar(config, rest),
        "list" => memory_list(config, rest),
        "show" => memory_show(config, rest),
        "audit" => memory_audit(config, rest),
        "approve" => memory_review(config, rest, ReviewCommand::Approve),
        "reject" => memory_review(config, rest, ReviewCommand::Reject),
        "archive" => memory_review(config, rest, ReviewCommand::Archive),
        "edit" => memory_edit(config, rest),
        "restore" => memory_restore(config, rest),
        _ => Err(format!("unknown memory command: {command}")),
    }
}

fn memory_similar(config: CliConfig, args: &[String]) -> CliResult<String> {
    let mut content = None;
    let mut kind = None;
    let mut threshold = None;
    let mut limit = None;
    let mut json_output = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--content" => {
                content = Some(required_value(args, index, "--content")?);
                index += 2;
            }
            "--kind" => {
                kind = Some(parse_kind(&required_value(args, index, "--kind")?)?);
                index += 2;
            }
            "--threshold" => {
                let score = required_value(args, index, "--threshold")?
                    .parse::<u8>()
                    .map_err(|_| "invalid --threshold".to_string())?;
                if score > 100 {
                    return Err("invalid --threshold".to_string());
                }
                threshold = Some(score);
                index += 2;
            }
            "--limit" => {
                limit = Some(
                    required_value(args, index, "--limit")?
                        .parse::<usize>()
                        .map_err(|_| "invalid --limit".to_string())?,
                );
                index += 2;
            }
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown memory similar option: {flag}")),
        }
    }

    let content = content.ok_or_else(|| "missing required --content".to_string())?;
    let kind = kind.ok_or_else(|| "missing required --kind".to_string())?;
    let mut query = MemorySimilarityQuery::new(config.project.clone(), kind, content);
    if let Some(threshold) = threshold {
        query = query.with_min_score(threshold);
    }
    if let Some(limit) = limit {
        query = query.with_limit(limit);
    }

    let store = open_memory_store(&config)?;
    let records = store
        .similar_active(query)
        .map_err(|error| error.to_string())?;
    if json_output {
        let items = records.iter().map(similar_memory_json).collect::<Vec<_>>();
        return serde_json::to_string(&items).map_err(|error| error.to_string());
    }

    let mut output = String::new();
    for memory in records {
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            memory.entry().id(),
            status_label(memory.entry().status()),
            kind_label(memory.entry().kind()),
            memory.score(),
            memory.entry().content()
        ));
    }
    Ok(output)
}

fn memory_search(config: CliConfig, args: &[String]) -> CliResult<String> {
    let mut query = MemoryQuery::new(config.project.clone());
    let mut json_output = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--text" => {
                query = query.with_text(required_value(args, index, "--text")?);
                index += 2;
            }
            "--kind" => {
                query = query.with_kind(parse_kind(&required_value(args, index, "--kind")?)?);
                index += 2;
            }
            "--tag" => {
                query = query.with_tag(required_value(args, index, "--tag")?);
                index += 2;
            }
            "--limit" => {
                let limit = required_value(args, index, "--limit")?
                    .parse::<usize>()
                    .map_err(|_| "invalid --limit".to_string())?;
                query = query.with_limit(limit);
                index += 2;
            }
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown memory search option: {flag}")),
        }
    }

    let store = open_memory_store(&config)?;
    let records = store.retrieve(query).map_err(|error| error.to_string())?;
    if json_output {
        let items = records
            .iter()
            .map(retrieved_memory_json)
            .collect::<Vec<_>>();
        return serde_json::to_string(&items).map_err(|error| error.to_string());
    }

    let mut output = String::new();
    for memory in records {
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            memory.entry().id(),
            status_label(memory.entry().status()),
            kind_label(memory.entry().kind()),
            memory.reason(),
            memory.entry().content()
        ));
    }
    Ok(output)
}

fn memory_list(config: CliConfig, args: &[String]) -> CliResult<String> {
    let mut filter = MemoryListFilter::new(config.project.clone());
    let mut json_output = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--status" => {
                filter =
                    filter.with_status(parse_status(&required_value(args, index, "--status")?)?);
                index += 2;
            }
            "--kind" => {
                filter = filter.with_kind(parse_kind(&required_value(args, index, "--kind")?)?);
                index += 2;
            }
            "--tag" => {
                filter = filter.with_tag(required_value(args, index, "--tag")?);
                index += 2;
            }
            "--limit" => {
                let limit = required_value(args, index, "--limit")?
                    .parse::<usize>()
                    .map_err(|_| "invalid --limit".to_string())?;
                filter = filter.with_limit(limit);
                index += 2;
            }
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown memory list option: {flag}")),
        }
    }

    let store = open_memory_store(&config)?;
    let records = store.list(filter).map_err(|error| error.to_string())?;
    if json_output {
        let items = records.iter().map(memory_record_json).collect::<Vec<_>>();
        return serde_json::to_string(&items).map_err(|error| error.to_string());
    }

    let mut output = String::new();
    for record in records {
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            record.entry().id(),
            status_label(record.entry().status()),
            kind_label(record.entry().kind()),
            record.entry().content()
        ));
    }
    Ok(output)
}

fn memory_show(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some(id) = args.first() else {
        return Err("memory show expects exactly one memory id".to_string());
    };
    let mut json_output = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown memory show option: {flag}")),
        }
    }

    let store = open_memory_store(&config)?;
    let record = store.get(id).map_err(|error| error.to_string())?;
    if json_output {
        return serde_json::to_string(&memory_record_json(&record))
            .map_err(|error| error.to_string());
    }

    let mut output = format!(
        "id: {}\nstatus: {}\nkind: {}\ntags: {}\n",
        record.entry().id(),
        status_label(record.entry().status()),
        kind_label(record.entry().kind()),
        record.entry().tags().join(",")
    );
    if let Some(source) = record.entry().source() {
        output.push_str(&format!(
            "source_session: {}\nsource_turn: {}\n",
            source.session_id().as_str(),
            source.turn_id().as_str()
        ));
    }
    output.push_str(&format!("content: {}\n", record.entry().content()));

    Ok(output)
}

fn memory_record_json(record: &codel00p_memory::MemoryRecord) -> Value {
    memory_entry_json(record.entry())
}

fn retrieved_memory_json(memory: &codel00p_memory::RetrievedMemory) -> Value {
    let mut item = memory_entry_json(memory.entry());
    item["reason"] = json!(memory.reason());
    item
}

fn similar_memory_json(memory: &codel00p_memory::SimilarMemory) -> Value {
    let mut item = memory_entry_json(memory.entry());
    item["score"] = json!(memory.score());
    item
}

fn memory_entry_json(entry: &codel00p_protocol::MemoryEntry) -> Value {
    let mut item = json!({
        "id": entry.id(),
        "status": status_label(entry.status()),
        "kind": kind_label(entry.kind()),
        "content": entry.content(),
        "tags": entry.tags(),
    });
    if let Some(source) = entry.source() {
        item["source"] = json!({
            "session_id": source.session_id().as_str(),
            "turn_id": source.turn_id().as_str(),
        });
    }
    item
}

fn memory_audit(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some(id) = args.first() else {
        return Err("missing memory id".to_string());
    };
    let mut json_output = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown memory audit option: {flag}")),
        }
    }

    let store = open_memory_store(&config)?;
    let audit = store.audit_log(id).map_err(|error| error.to_string())?;
    if json_output {
        let items = audit.iter().map(audit_event_json).collect::<Vec<_>>();
        return serde_json::to_string(&items).map_err(|error| error.to_string());
    }

    let mut output = String::new();
    for event in audit {
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            event.sequence(),
            audit_action_label(event.action()),
            event.actor(),
            event.reason().unwrap_or("")
        ));
    }
    Ok(output)
}

fn audit_event_json(event: &codel00p_memory::MemoryAuditEvent) -> Value {
    let mut item = json!({
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
}

enum ReviewCommand {
    Approve,
    Reject,
    Archive,
}

fn memory_review(config: CliConfig, args: &[String], command: ReviewCommand) -> CliResult<String> {
    let Some(id) = args.first() else {
        return Err("missing memory id".to_string());
    };
    let mut actor = None;
    let mut reason = None;
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--actor" => {
                actor = Some(required_value(args, index, "--actor")?);
                index += 2;
            }
            "--reason" => {
                reason = Some(required_value(args, index, "--reason")?);
                index += 2;
            }
            flag => return Err(format!("unknown review option: {flag}")),
        }
    }

    let actor = actor.ok_or_else(|| "missing required --actor".to_string())?;
    let decision = match command {
        ReviewCommand::Approve => ReviewDecision::approve(actor),
        ReviewCommand::Reject => ReviewDecision::reject(
            actor,
            reason.ok_or_else(|| "missing required --reason".to_string())?,
        ),
        ReviewCommand::Archive => ReviewDecision::archive(
            actor,
            reason.ok_or_else(|| "missing required --reason".to_string())?,
        ),
    };

    let mut store = open_memory_store(&config)?;
    let record = store
        .review(id, decision)
        .map_err(|error| error.to_string())?;

    Ok(format!(
        "{}\t{}\n",
        record.entry().id(),
        status_label(record.entry().status())
    ))
}

fn memory_edit(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some(id) = args.first() else {
        return Err("missing memory id".to_string());
    };
    let mut actor = None;
    let mut content = None;
    let mut reason = None;
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--actor" => {
                actor = Some(required_value(args, index, "--actor")?);
                index += 2;
            }
            "--content" => {
                content = Some(required_value(args, index, "--content")?);
                index += 2;
            }
            "--reason" => {
                reason = Some(required_value(args, index, "--reason")?);
                index += 2;
            }
            flag => return Err(format!("unknown memory edit option: {flag}")),
        }
    }

    let actor = actor.ok_or_else(|| "missing required --actor".to_string())?;
    let content = content.ok_or_else(|| "missing required --content".to_string())?;
    let mut edit = MemoryEdit::replace_content(actor, content);
    if let Some(reason) = reason {
        edit = edit.with_reason(reason);
    }

    let mut store = open_memory_store(&config)?;
    let record = store.edit(id, edit).map_err(|error| error.to_string())?;

    Ok(format!(
        "{}\t{}\n",
        record.entry().id(),
        status_label(record.entry().status())
    ))
}

fn memory_restore(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some(id) = args.first() else {
        return Err("missing memory id".to_string());
    };
    let mut sequence = None;
    let mut actor = None;
    let mut reason = None;
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--sequence" => {
                sequence = Some(
                    required_value(args, index, "--sequence")?
                        .parse::<u64>()
                        .map_err(|_| "invalid --sequence".to_string())?,
                );
                index += 2;
            }
            "--actor" => {
                actor = Some(required_value(args, index, "--actor")?);
                index += 2;
            }
            "--reason" => {
                reason = Some(required_value(args, index, "--reason")?);
                index += 2;
            }
            flag => return Err(format!("unknown memory restore option: {flag}")),
        }
    }

    let sequence = sequence.ok_or_else(|| "missing required --sequence".to_string())?;
    let actor = actor.ok_or_else(|| "missing required --actor".to_string())?;

    let mut store = open_memory_store(&config)?;
    let audit = store.audit_log(id).map_err(|error| error.to_string())?;
    let previous_content = audit
        .iter()
        .find(|event| event.sequence() == sequence)
        .ok_or_else(|| format!("memory audit sequence not found: {sequence}"))?
        .previous_content()
        .ok_or_else(|| format!("memory audit sequence {sequence} has no previous content"))?
        .to_string();

    let mut edit = MemoryEdit::replace_content(actor, previous_content);
    edit = edit.with_reason(reason.unwrap_or_else(|| format!("restore audit sequence {sequence}")));

    let record = store.edit(id, edit).map_err(|error| error.to_string())?;

    Ok(format!(
        "{}\t{}\n",
        record.entry().id(),
        status_label(record.entry().status())
    ))
}

fn parse_status(value: &str) -> CliResult<MemoryStatus> {
    match value {
        "candidate" => Ok(MemoryStatus::Candidate),
        "approved" => Ok(MemoryStatus::Approved),
        "rejected" => Ok(MemoryStatus::Rejected),
        "archived" => Ok(MemoryStatus::Archived),
        _ => Err(format!("unknown memory status: {value}")),
    }
}

fn parse_kind(value: &str) -> CliResult<MemoryKind> {
    match value {
        "architecture" => Ok(MemoryKind::Architecture),
        "convention" => Ok(MemoryKind::Convention),
        "workflow" => Ok(MemoryKind::Workflow),
        "decision" => Ok(MemoryKind::Decision),
        "deployment" => Ok(MemoryKind::Deployment),
        "troubleshooting" => Ok(MemoryKind::Troubleshooting),
        _ => Err(format!("unknown memory kind: {value}")),
    }
}

fn status_label(status: MemoryStatus) -> &'static str {
    match status {
        MemoryStatus::Candidate => "candidate",
        MemoryStatus::Approved => "approved",
        MemoryStatus::Rejected => "rejected",
        MemoryStatus::Archived => "archived",
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

fn audit_action_label(action: codel00p_memory::MemoryAuditAction) -> &'static str {
    match action {
        codel00p_memory::MemoryAuditAction::CandidateCreated => "candidate_created",
        codel00p_memory::MemoryAuditAction::Approved => "approved",
        codel00p_memory::MemoryAuditAction::Rejected => "rejected",
        codel00p_memory::MemoryAuditAction::Archived => "archived",
        codel00p_memory::MemoryAuditAction::Edited => "edited",
    }
}
