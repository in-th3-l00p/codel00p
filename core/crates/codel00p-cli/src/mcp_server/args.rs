//! Argument extraction and enum parsing for MCP tool calls.

use super::*;

pub(super) fn required_string<'a>(arguments: &'a Value, key: &str) -> Result<&'a str, String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing required argument `{key}`"))
}

pub(super) fn optional_string<'a>(arguments: &'a Value, key: &str) -> Option<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

pub(super) fn optional_usize(arguments: &Value, key: &str) -> Result<Option<usize>, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(None);
    };
    value
        .as_u64()
        .map(|value| Some(value as usize))
        .ok_or_else(|| format!("argument `{key}` must be a positive integer"))
}

pub(super) fn required_u64(arguments: &Value, key: &str) -> Result<u64, String> {
    arguments
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("missing required argument `{key}`"))
}

pub(super) fn optional_string_array<'a>(
    arguments: &'a Value,
    key: &str,
) -> Result<Vec<&'a str>, String> {
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

pub(super) fn parse_turn_id(value: &str) -> Result<TurnId, String> {
    serde_json::from_value(Value::String(value.to_string()))
        .map_err(|error| format!("invalid turn_id: {error}"))
}

pub(super) fn parse_status(value: &str) -> Result<MemoryStatus, String> {
    match value {
        "candidate" => Ok(MemoryStatus::Candidate),
        "approved" => Ok(MemoryStatus::Approved),
        "rejected" => Ok(MemoryStatus::Rejected),
        "archived" => Ok(MemoryStatus::Archived),
        _ => Err(format!("unknown memory status: {value}")),
    }
}

pub(super) fn parse_kind(value: &str) -> Result<MemoryKind, String> {
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

pub(super) fn parse_sensitivity(value: &str) -> Result<MemorySensitivity, String> {
    match value {
        "normal" => Ok(MemorySensitivity::Normal),
        "sensitive" => Ok(MemorySensitivity::Sensitive),
        _ => Err(format!("unknown memory sensitivity: {value}")),
    }
}

pub(super) fn parse_visibility(value: &str) -> Result<MemoryVisibility, String> {
    match value {
        "private" => Ok(MemoryVisibility::Private),
        "project" => Ok(MemoryVisibility::Project),
        "team" => Ok(MemoryVisibility::Team),
        "org" => Ok(MemoryVisibility::Org),
        _ => Err(format!("unknown memory visibility: {value}")),
    }
}
