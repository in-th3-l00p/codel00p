//! MCP resource readers for memory records and session replays.

use super::*;

pub(super) fn read_resource(config: &CliConfig, params: &Value) -> Result<Value, String> {
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
