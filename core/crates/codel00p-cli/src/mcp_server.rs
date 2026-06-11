use std::io;

use codel00p_mcp::{McpServerHandler, McpServerResponse, serve_stdio_server};
use codel00p_memory::{
    MemoryAuditAction, MemoryCandidateInput, MemoryEdit, MemoryListFilter, MemoryQuery,
    MemoryRepository, MemorySimilarityQuery, MemoryStalenessQuery, ReviewDecision,
};
use codel00p_protocol::{
    MemoryKind, MemorySensitivity, MemorySource, MemoryStatus, SessionMessage, SessionRole, TurnId,
};
use codel00p_session::{SessionRecord, SessionStore, SessionStoreError};
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
    let stdout = io::stdout();
    let mut handler = Codel00pMcpServer { config };
    serve_stdio_server(stdin.lock(), stdout, &mut handler).map_err(|error| error.to_string())
}

struct Codel00pMcpServer {
    config: CliConfig,
}

impl McpServerHandler for Codel00pMcpServer {
    fn handle_method(&mut self, method: &str, params: &Value) -> Result<McpServerResponse, String> {
        dispatch_json_rpc(&self.config, method, params)
    }
}

fn dispatch_json_rpc(
    config: &CliConfig,
    method: &str,
    params: &Value,
) -> Result<McpServerResponse, String> {
    let result = match method {
        "initialize" => json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {
                "tools": {},
                "resources": {
                    "subscribe": true
                }
            },
            "serverInfo": {
                "name": "codel00p",
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
        "tools/list" => json!({ "tools": mcp_tools() }),
        "tools/call" => return call_tool(config, params),
        "resources/list" => json!({
            "resources": [],
            "resourceTemplates": mcp_resource_templates()
        }),
        "resources/read" => read_resource(config, params)?,
        _ => return Err(format!("unsupported method: {method}")),
    };
    Ok(McpServerResponse::new(result))
}

fn mcp_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "memory_similar",
            "description": "Score active near-duplicate codel00p project memory.",
            "inputSchema": {
                "type": "object",
                "required": ["content", "kind"],
                "properties": {
                    "content": { "type": "string" },
                    "kind": { "type": "string" },
                    "threshold": { "type": "integer", "minimum": 0, "maximum": 100 },
                    "limit": { "type": "integer", "minimum": 1 }
                }
            }
        }),
        json!({
            "name": "memory_stale",
            "description": "Find approved codel00p project memory likely superseded by newer active memory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": { "type": "string" },
                    "threshold": { "type": "integer", "minimum": 0, "maximum": 100 },
                    "limit": { "type": "integer", "minimum": 1 }
                }
            }
        }),
        json!({
            "name": "memory_search",
            "description": "Search approved codel00p project memory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string" },
                    "kind": { "type": "string" },
                    "sensitivity": { "type": "string" },
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
                    "sensitivity": { "type": "string" },
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
            "name": "memory_audit",
            "description": "Show audit history for one codel00p memory record.",
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
                    "sensitivity": { "type": "string" },
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
            "name": "memory_edit",
            "description": "Edit one codel00p project memory record.",
            "inputSchema": {
                "type": "object",
                "required": ["id", "actor", "content"],
                "properties": {
                    "id": { "type": "string" },
                    "actor": { "type": "string" },
                    "content": { "type": "string" },
                    "reason": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "memory_restore",
            "description": "Restore one codel00p project memory record from an edit audit sequence.",
            "inputSchema": {
                "type": "object",
                "required": ["id", "sequence", "actor"],
                "properties": {
                    "id": { "type": "string" },
                    "sequence": { "type": "integer", "minimum": 1 },
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

fn mcp_resource_templates() -> Vec<Value> {
    vec![
        json!({
            "uriTemplate": "codel00p://memory/{id}",
            "name": "codel00p memory record",
            "description": "Read one codel00p project memory record as JSON.",
            "mimeType": "application/json"
        }),
        json!({
            "uriTemplate": "codel00p://sessions/{session_id}",
            "name": "codel00p session replay",
            "description": "Read one codel00p agent session replay as JSON.",
            "mimeType": "application/json"
        }),
    ]
}

fn call_tool(config: &CliConfig, params: &Value) -> Result<McpServerResponse, String> {
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

fn read_resource(config: &CliConfig, params: &Value) -> Result<Value, String> {
    let uri = required_string(params, "uri")?;
    let text = if let Some(memory_id) = uri.strip_prefix("codel00p://memory/") {
        let store = open_memory_store(config)?;
        let record = store.get(memory_id).map_err(|error| error.to_string())?;
        serde_json::to_string(&memory_record_json(&record)).map_err(|error| error.to_string())?
    } else if let Some(session_id) = uri.strip_prefix("codel00p://sessions/") {
        session_resource(config, session_id)?
    } else {
        return Err(format!("unsupported codel00p resource uri: {uri}"));
    };
    Ok(json!({
        "contents": [
            {
                "uri": uri,
                "mimeType": "application/json",
                "text": text
            }
        ]
    }))
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
    let source = MemorySource::turn(
        parse_session_id(required_string(arguments, "session_id")?)?,
        parse_turn_id(required_string(arguments, "turn_id")?)?,
    );
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

fn session_resource(config: &CliConfig, session_id: &str) -> Result<String, String> {
    let session_id = parse_session_id(session_id)?;
    let store = open_session_store(config)?;
    let records = match store.replay(&session_id) {
        Ok(records) => records,
        Err(SessionStoreError::SessionNotFound { .. }) => Vec::new(),
        Err(error) => return Err(error.to_string()),
    };
    serde_json::to_string(&session_records_json(&records)).map_err(|error| error.to_string())
}

fn session_records_json(records: &[codel00p_session::PersistedSessionRecord]) -> Vec<Value> {
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

fn stale_memory_json(memory: &codel00p_memory::StaleMemory) -> Value {
    let mut item = memory_entry_json(memory.entry());
    item["score"] = json!(memory.score());
    item["newer"] = memory_entry_json(memory.newer_entry());
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
        item["source"] = json!({
            "session_id": source.session_id().as_str(),
            "turn_id": source.turn_id().as_str(),
        });
        item["source_uri"] = json!(source_uri(source));
    }
    item
}

fn source_uri(source: &MemorySource) -> String {
    format!("codel00p://sessions/{}", source.session_id().as_str())
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

fn required_u64(arguments: &Value, key: &str) -> Result<u64, String> {
    arguments
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("missing required argument `{key}`"))
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

fn parse_sensitivity(value: &str) -> Result<MemorySensitivity, String> {
    match value {
        "normal" => Ok(MemorySensitivity::Normal),
        "sensitive" => Ok(MemorySensitivity::Sensitive),
        _ => Err(format!("unknown memory sensitivity: {value}")),
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

fn sensitivity_label(sensitivity: MemorySensitivity) -> &'static str {
    match sensitivity {
        MemorySensitivity::Normal => "normal",
        MemorySensitivity::Sensitive => "sensitive",
    }
}

fn audit_action_label(action: MemoryAuditAction) -> &'static str {
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
