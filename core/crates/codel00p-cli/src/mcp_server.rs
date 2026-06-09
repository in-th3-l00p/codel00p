use std::io::{self, BufRead, Write};

use codel00p_memory::{
    MemoryCandidateInput, MemoryListFilter, MemoryQuery, MemoryRepository, ReviewDecision,
};
use codel00p_protocol::{
    MemoryKind, MemorySource, MemoryStatus, SessionMessage, SessionRole, TurnId,
};
use codel00p_session::{SessionRecord, SessionStore};
use serde_json::{Value, json};

use crate::config::{
    CliConfig, CliResult, open_memory_store, open_session_store, parse_session_id, required_value,
};
use crate::connector_permissions::{
    forget_decision, list_decisions, parse_scope_label, scope_label,
    status_label as connector_status_label,
};

pub fn run(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("missing mcp command".to_string());
    };

    match command.as_str() {
        "serve" => {
            if !rest.is_empty() {
                return Err("mcp serve does not accept arguments".to_string());
            }
            serve_stdio(config)?;
            Ok(String::new())
        }
        "permissions" => permissions(config, rest),
        _ => Err(format!("unknown mcp command: {command}")),
    }
}

fn permissions(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("missing mcp permissions command".to_string());
    };

    match command.as_str() {
        "list" => permissions_list(&config, rest),
        "forget" => permissions_forget(&config, rest),
        _ => Err(format!("unknown mcp permissions command: {command}")),
    }
}

fn permissions_list(config: &CliConfig, args: &[String]) -> CliResult<String> {
    if !args.is_empty() {
        return Err("mcp permissions list does not accept arguments".to_string());
    }
    let mut output = String::new();
    for decision in list_decisions(config)? {
        output.push_str(&format!(
            "{}\t{}\t{}\n",
            decision.tool_name,
            scope_label(decision.scope),
            connector_status_label(decision.status)
        ));
    }
    Ok(output)
}

fn permissions_forget(config: &CliConfig, args: &[String]) -> CliResult<String> {
    let Some(tool_name) = args.first() else {
        return Err("mcp permissions forget expects a tool name".to_string());
    };
    let mut scope = "external_connector".to_string();
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--scope" => {
                scope = required_value(args, index, "--scope")?;
                index += 2;
            }
            value => return Err(format!("unknown mcp permissions forget option: {value}")),
        }
    }
    let scope = parse_scope_label(&scope)?;
    let status = if forget_decision(config, tool_name, scope)? {
        "forgot"
    } else {
        "missing"
    };
    Ok(format!("{status}\t{tool_name}\t{}\n", scope_label(scope)))
}

fn serve_stdio(config: CliConfig) -> CliResult<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| error.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = serde_json::from_str(&line)
            .map_err(|error| format!("invalid json-rpc request: {error}"))?;
        if let Some(response) = handle_json_rpc(&config, request) {
            writeln!(
                stdout,
                "{}",
                serde_json::to_string(&response).map_err(|error| error.to_string())?
            )
            .map_err(|error| error.to_string())?;
            stdout.flush().map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn handle_json_rpc(config: &CliConfig, request: Value) -> Option<Value> {
    let method = request.get("method").and_then(Value::as_str)?;
    let id = request.get("id").cloned();
    id.as_ref()?;
    let id = id.expect("checked id");
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2025-06-18",
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": "codel00p",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
        "tools/list" => Ok(json!({ "tools": mcp_tools() })),
        "tools/call" => call_tool(config, &params),
        _ => Err(format!("unsupported method: {method}")),
    };

    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(message) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32000,
                "message": message
            }
        }),
    })
}

fn mcp_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "memory_search",
            "description": "Search approved codel00p project memory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string" },
                    "kind": { "type": "string" },
                    "tag": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1 }
                }
            }
        }),
        json!({
            "name": "memory_list",
            "description": "List codel00p project memory records.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status": { "type": "string" },
                    "kind": { "type": "string" },
                    "tag": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1 }
                }
            }
        }),
        json!({
            "name": "memory_show",
            "description": "Show one codel00p memory record by id.",
            "inputSchema": {
                "type": "object",
                "required": ["id"],
                "properties": {
                    "id": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "memory_create_candidate",
            "description": "Create candidate codel00p project memory for review.",
            "inputSchema": {
                "type": "object",
                "required": ["id", "kind", "content", "session_id", "turn_id"],
                "properties": {
                    "id": { "type": "string" },
                    "kind": { "type": "string" },
                    "content": { "type": "string" },
                    "session_id": { "type": "string" },
                    "turn_id": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } }
                }
            }
        }),
        json!({
            "name": "memory_approve",
            "description": "Approve one codel00p project memory candidate.",
            "inputSchema": {
                "type": "object",
                "required": ["id", "actor"],
                "properties": {
                    "id": { "type": "string" },
                    "actor": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "memory_reject",
            "description": "Reject one codel00p project memory candidate.",
            "inputSchema": {
                "type": "object",
                "required": ["id", "actor", "reason"],
                "properties": {
                    "id": { "type": "string" },
                    "actor": { "type": "string" },
                    "reason": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "memory_archive",
            "description": "Archive one codel00p project memory record.",
            "inputSchema": {
                "type": "object",
                "required": ["id", "actor", "reason"],
                "properties": {
                    "id": { "type": "string" },
                    "actor": { "type": "string" },
                    "reason": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "session_show",
            "description": "Replay one codel00p agent session by id.",
            "inputSchema": {
                "type": "object",
                "required": ["session_id"],
                "properties": {
                    "session_id": { "type": "string" }
                }
            }
        }),
    ]
}

fn call_tool(config: &CliConfig, params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "tools/call omitted name".to_string())?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let text = match name {
        "memory_search" => memory_search(config, &arguments)?,
        "memory_list" => memory_list(config, &arguments)?,
        "memory_show" => memory_show(config, &arguments)?,
        "memory_create_candidate" => memory_create_candidate(config, &arguments)?,
        "memory_approve" => memory_review(config, &arguments, MemoryReviewAction::Approve)?,
        "memory_reject" => memory_review(config, &arguments, MemoryReviewAction::Reject)?,
        "memory_archive" => memory_review(config, &arguments, MemoryReviewAction::Archive)?,
        "session_show" => session_show(config, &arguments)?,
        _ => return Err(format!("unknown codel00p MCP tool: {name}")),
    };
    Ok(json!({
        "content": [
            { "type": "text", "text": text }
        ],
        "isError": false
    }))
}

fn memory_search(config: &CliConfig, arguments: &Value) -> Result<String, String> {
    let mut query = MemoryQuery::new(config.project.clone());
    if let Some(text) = optional_string(arguments, "text") {
        query = query.with_text(text);
    }
    if let Some(kind) = optional_string(arguments, "kind") {
        query = query.with_kind(parse_kind(kind)?);
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
        .map(|record| {
            json!({
                "id": record.entry().id(),
                "kind": kind_label(record.entry().kind()),
                "content": record.entry().content(),
                "reason": record.reason(),
                "tags": record.entry().tags(),
            })
        })
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
    if let Some(tag) = optional_string(arguments, "tag") {
        filter = filter.with_tag(tag);
    }
    if let Some(limit) = optional_usize(arguments, "limit")? {
        filter = filter.with_limit(limit);
    }

    let store = open_memory_store(config)?;
    let records = store.list(filter).map_err(|error| error.to_string())?;
    let items = records
        .iter()
        .map(|record| {
            json!({
                "id": record.entry().id(),
                "status": status_label(record.entry().status()),
                "kind": kind_label(record.entry().kind()),
                "content": record.entry().content(),
                "tags": record.entry().tags(),
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&items).map_err(|error| error.to_string())
}

fn memory_show(config: &CliConfig, arguments: &Value) -> Result<String, String> {
    let id = required_string(arguments, "id")?;
    let store = open_memory_store(config)?;
    let record = store.get(id).map_err(|error| error.to_string())?;
    serde_json::to_string(&memory_record_json(&record)).map_err(|error| error.to_string())
}

fn memory_create_candidate(config: &CliConfig, arguments: &Value) -> Result<String, String> {
    let source = MemorySource::turn(
        parse_session_id(required_string(arguments, "session_id")?)?,
        parse_turn_id(required_string(arguments, "turn_id")?)?,
    );
    let mut input = MemoryCandidateInput::new(
        required_string(arguments, "id")?,
        config.project.clone(),
        parse_kind(required_string(arguments, "kind")?)?,
        required_string(arguments, "content")?,
        source,
    );
    for tag in optional_string_array(arguments, "tags")? {
        input = input.with_tag(tag);
    }

    let mut store = open_memory_store(config)?;
    let record = store
        .create_candidate(input)
        .map_err(|error| error.to_string())?;
    serde_json::to_string(&memory_record_json(&record)).map_err(|error| error.to_string())
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
) -> Result<String, String> {
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
    serde_json::to_string(&memory_record_json(&record)).map_err(|error| error.to_string())
}

fn session_show(config: &CliConfig, arguments: &Value) -> Result<String, String> {
    let session_id = parse_session_id(required_string(arguments, "session_id")?)?;
    let store = open_session_store(config)?;
    let records = store
        .replay(&session_id)
        .map_err(|error| error.to_string())?;
    let items = records
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
        .collect::<Vec<_>>();
    serde_json::to_string(&items).map_err(|error| error.to_string())
}

fn memory_record_json(record: &codel00p_memory::MemoryRecord) -> Value {
    json!({
        "id": record.entry().id(),
        "status": status_label(record.entry().status()),
        "kind": kind_label(record.entry().kind()),
        "content": record.entry().content(),
        "tags": record.entry().tags(),
    })
}

fn required_string<'a>(arguments: &'a Value, key: &str) -> Result<&'a str, String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing required argument `{key}`"))
}

fn optional_string<'a>(arguments: &'a Value, key: &str) -> Option<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn optional_usize(arguments: &Value, key: &str) -> Result<Option<usize>, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(None);
    };
    value
        .as_u64()
        .map(|value| Some(value as usize))
        .ok_or_else(|| format!("argument `{key}` must be a positive integer"))
}

fn optional_string_array<'a>(arguments: &'a Value, key: &str) -> Result<Vec<&'a str>, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("argument `{key}` must be an array of strings"))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| format!("argument `{key}` must be an array of strings"))
        })
        .collect()
}

fn parse_turn_id(value: &str) -> Result<TurnId, String> {
    serde_json::from_value(Value::String(value.to_string()))
        .map_err(|error| format!("invalid turn_id: {error}"))
}

fn parse_status(value: &str) -> Result<MemoryStatus, String> {
    match value {
        "candidate" => Ok(MemoryStatus::Candidate),
        "approved" => Ok(MemoryStatus::Approved),
        "rejected" => Ok(MemoryStatus::Rejected),
        "archived" => Ok(MemoryStatus::Archived),
        _ => Err(format!("unknown memory status: {value}")),
    }
}

fn parse_kind(value: &str) -> Result<MemoryKind, String> {
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
