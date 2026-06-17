use std::io;

use codel00p_mcp::{McpServerHandler, McpServerResponse, serve_stdio_server};
use codel00p_memory::{
    MemoryAuditAction, MemoryCandidateInput, MemoryEdit, MemoryListFilter, MemoryMerge,
    MemoryQualityQuery, MemoryQuery, MemoryRepository, MemoryRetrievalQuery, MemorySimilarityQuery,
    MemorySplit, MemoryStalenessQuery, ReviewDecision,
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

mod args;
mod descriptors;
mod permissions;
mod resources;
mod serializers;
mod server;
mod tools;

use args::{
    optional_string, optional_string_array, optional_usize, parse_kind, parse_sensitivity,
    parse_status, parse_turn_id, required_string, required_u64,
};
use descriptors::{mcp_resource_templates, mcp_tools};
use permissions::permissions;
use resources::read_resource;
use serializers::{
    audit_action_label, memory_record_json, quality_memory_json, ranked_memory_json,
    retrieved_memory_json, session_records_json, similar_memory_json, stale_memory_json,
};
use server::serve_stdio;
use tools::call_tool;

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
