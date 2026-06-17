use codel00p_memory::{MemoryEdit, MemoryMerge, MemoryRepository, MemorySplit, ReviewDecision};

use crate::config::{CliConfig, CliResult, open_memory_store, required_value};

use super::{
    json::{audit_event_json, memory_record_json},
    parse::{audit_action_label, status_label},
};

pub(super) enum ReviewCommand {
    Approve,
    Reject,
    Archive,
}

pub(super) fn memory_audit(config: CliConfig, args: &[String]) -> CliResult<String> {
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

pub(super) fn memory_review(
    config: CliConfig,
    args: &[String],
    command: ReviewCommand,
) -> CliResult<String> {
    let Some(id) = args.first() else {
        return Err("missing memory id".to_string());
    };
    let mut actor = None;
    let mut reason = None;
    let mut json_output = false;
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
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown review option: {flag}")),
        }
    }

    let actor = actor.unwrap_or_else(crate::actor::infer_actor);
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
    if json_output {
        return serde_json::to_string(&memory_record_json(&record))
            .map_err(|error| error.to_string());
    }

    Ok(format!(
        "{}\t{}\n",
        record.entry().id(),
        status_label(record.entry().status())
    ))
}

pub(super) fn memory_edit(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some(id) = args.first() else {
        return Err("missing memory id".to_string());
    };
    let mut actor = None;
    let mut content = None;
    let mut reason = None;
    let mut json_output = false;
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
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown memory edit option: {flag}")),
        }
    }

    let actor = actor.unwrap_or_else(crate::actor::infer_actor);
    let content = content.ok_or_else(|| "missing required --content".to_string())?;
    let mut edit = MemoryEdit::replace_content(actor, content);
    if let Some(reason) = reason {
        edit = edit.with_reason(reason);
    }

    let mut store = open_memory_store(&config)?;
    let record = store.edit(id, edit).map_err(|error| error.to_string())?;
    if json_output {
        return serde_json::to_string(&memory_record_json(&record))
            .map_err(|error| error.to_string());
    }

    Ok(format!(
        "{}\t{}\n",
        record.entry().id(),
        status_label(record.entry().status())
    ))
}

pub(super) fn memory_merge(config: CliConfig, args: &[String]) -> CliResult<String> {
    let source_id = args
        .first()
        .ok_or_else(|| "missing source memory id".to_string())?;
    let target_id = match args.get(1) {
        Some(value) if !value.starts_with("--") => value,
        _ => return Err("missing target memory id".to_string()),
    };

    let mut actor = None;
    let mut reason = None;
    let mut json_output = false;
    let mut index = 2;

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
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown memory merge option: {flag}")),
        }
    }

    let actor = actor.unwrap_or_else(crate::actor::infer_actor);
    let mut merge = MemoryMerge::new(actor);
    if let Some(reason) = reason {
        merge = merge.with_reason(reason);
    }

    let mut store = open_memory_store(&config)?;
    let record = store
        .merge(source_id, target_id, merge)
        .map_err(|error| error.to_string())?;
    if json_output {
        return serde_json::to_string(&memory_record_json(&record))
            .map_err(|error| error.to_string());
    }

    Ok(format!(
        "{}\t{}\n",
        record.entry().id(),
        status_label(record.entry().status())
    ))
}

pub(super) fn memory_split(config: CliConfig, args: &[String]) -> CliResult<String> {
    let source_id = args
        .first()
        .ok_or_else(|| "missing source memory id".to_string())?;
    let new_id = match args.get(1) {
        Some(value) if !value.starts_with("--") => value,
        _ => return Err("missing new memory id".to_string()),
    };

    let mut actor = None;
    let mut content = None;
    let mut source_content = None;
    let mut reason = None;
    let mut json_output = false;
    let mut index = 2;

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
            "--source-content" => {
                source_content = Some(required_value(args, index, "--source-content")?);
                index += 2;
            }
            "--reason" => {
                reason = Some(required_value(args, index, "--reason")?);
                index += 2;
            }
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown memory split option: {flag}")),
        }
    }

    let actor = actor.unwrap_or_else(crate::actor::infer_actor);
    let content = content.ok_or_else(|| "missing required --content".to_string())?;
    let mut split = MemorySplit::new(actor, new_id.clone(), content);
    if let Some(sc) = source_content {
        split = split.with_updated_source_content(sc);
    }
    if let Some(reason) = reason {
        split = split.with_reason(reason);
    }

    let mut store = open_memory_store(&config)?;
    let record = store
        .split(source_id, split)
        .map_err(|error| error.to_string())?;
    if json_output {
        return serde_json::to_string(&memory_record_json(&record))
            .map_err(|error| error.to_string());
    }

    Ok(format!(
        "{}\t{}\n",
        record.entry().id(),
        status_label(record.entry().status())
    ))
}

pub(super) fn memory_restore(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some(id) = args.first() else {
        return Err("missing memory id".to_string());
    };
    let mut sequence = None;
    let mut actor = None;
    let mut reason = None;
    let mut json_output = false;
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
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown memory restore option: {flag}")),
        }
    }

    let sequence = sequence.ok_or_else(|| "missing required --sequence".to_string())?;
    let actor = actor.unwrap_or_else(crate::actor::infer_actor);

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
    if json_output {
        return serde_json::to_string(&memory_record_json(&record))
            .map_err(|error| error.to_string());
    }

    Ok(format!(
        "{}\t{}\n",
        record.entry().id(),
        status_label(record.entry().status())
    ))
}
