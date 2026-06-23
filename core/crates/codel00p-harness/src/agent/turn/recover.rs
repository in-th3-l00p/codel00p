//! Salvage a tool call the model wrote as message *content* instead of emitting
//! a structured tool call.
//!
//! Smaller models sometimes "describe" a tool call in the assistant message — a
//! bare or fenced JSON object naming the tool and its arguments — rather than
//! returning it through the provider's tool-call channel. The turn loop would
//! otherwise see zero tool calls and end the turn, silently dropping the work.
//!
//! [`salvage_tool_call`] recovers that intent so the turn acts on it. It is
//! deliberately high-precision: the candidate (the first fenced code block, else
//! the whole trimmed message) must parse as a JSON object that names an
//! advertised tool. A normal prose answer does not parse as a bare JSON object,
//! so a genuine textual reply is never mistaken for a call.

use codel00p_protocol::ToolCall as ModelToolCall;
use serde_json::Value;

/// Keys a model might use for the tool name and its arguments when hand-rolling a
/// tool-call object.
const NAME_KEYS: &[&str] = &["name", "tool", "tool_name", "function"];
const ARG_KEYS: &[&str] = &["arguments", "input", "parameters", "args"];

/// Recover a single tool call from assistant `text`, or `None` if the text is not
/// unambiguously an attempted call to one of `advertised`.
pub(super) fn salvage_tool_call(text: &str, advertised: &[String]) -> Option<ModelToolCall> {
    let candidate = fenced_block(text).unwrap_or_else(|| text.trim().to_string());
    let parsed = serde_json::from_str::<Value>(&candidate).ok()?;
    let Value::Object(obj) = parsed else {
        return None;
    };

    let name = tool_name(&obj)?;
    if !advertised.iter().any(|advertised| advertised == &name) {
        return None;
    }
    let input = tool_args(&obj);
    Some(ModelToolCall::new(format!("salvaged-{name}"), name, input))
}

/// The inner content of the first fenced code block (```… ```), with an optional
/// language tag on the opening fence stripped. `None` when there is no closed
/// fence, so the caller falls back to the whole message.
fn fenced_block(text: &str) -> Option<String> {
    let after_open = text.split_once("```")?.1;
    // Drop the rest of the opening fence line (e.g. a `json` language tag).
    let body = match after_open.split_once('\n') {
        Some((_lang, rest)) => rest,
        None => after_open,
    };
    let inner = body.split_once("```")?.0;
    Some(inner.trim().to_string())
}

/// Extract the tool name from a hand-rolled call object: a string under one of
/// [`NAME_KEYS`], or a `function` object carrying its own `name`.
fn tool_name(obj: &serde_json::Map<String, Value>) -> Option<String> {
    for key in NAME_KEYS {
        match obj.get(*key) {
            Some(Value::String(name)) => return Some(name.clone()),
            // `function: { "name": "…" }` (OpenAI-ish shape).
            Some(Value::Object(function)) => {
                if let Some(Value::String(name)) = function.get("name") {
                    return Some(name.clone());
                }
            }
            _ => {}
        }
    }
    None
}

/// Extract the arguments object from a hand-rolled call object. A string value is
/// decoded as JSON when it yields an object; a missing/odd value yields `{}` so
/// the call still runs (and the tool reports any genuinely missing field).
fn tool_args(obj: &serde_json::Map<String, Value>) -> Value {
    for key in ARG_KEYS {
        match obj.get(*key) {
            Some(Value::Object(args)) => return Value::Object(args.clone()),
            Some(Value::String(text)) => {
                if let Ok(Value::Object(args)) = serde_json::from_str::<Value>(text) {
                    return Value::Object(args);
                }
            }
            _ => {}
        }
    }
    Value::Object(serde_json::Map::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn advertised() -> Vec<String> {
        vec!["apply_patch".to_string(), "read_file".to_string()]
    }

    #[test]
    fn salvages_a_fenced_json_tool_call() {
        let text = "Sure, here is the edit:\n```json\n{\"name\": \"apply_patch\", \
                    \"arguments\": {\"path\": \"a.rs\", \"find\": \"x\", \"replace\": \"y\"}}\n```";
        let call = salvage_tool_call(text, &advertised()).expect("salvaged");
        assert_eq!(call.name(), "apply_patch");
        assert_eq!(call.input()["path"], json!("a.rs"));
    }

    #[test]
    fn salvages_a_bare_json_object_message() {
        let text = "{\"tool\": \"read_file\", \"input\": {\"path\": \"x\"}}";
        let call = salvage_tool_call(text, &advertised()).expect("salvaged");
        assert_eq!(call.name(), "read_file");
        assert_eq!(call.input()["path"], json!("x"));
    }

    #[test]
    fn salvages_openai_function_shape_with_stringified_args() {
        let text = "```\n{\"function\": {\"name\": \"read_file\"}, \
                    \"arguments\": \"{\\\"path\\\": \\\"y\\\"}\"}\n```";
        let call = salvage_tool_call(text, &advertised()).expect("salvaged");
        assert_eq!(call.name(), "read_file");
        assert_eq!(call.input()["path"], json!("y"));
    }

    #[test]
    fn ignores_prose_that_merely_mentions_a_tool() {
        // A normal answer that names a tool but is not a JSON object: left alone.
        let text = "I would use apply_patch to change {path, find, replace} here.";
        assert!(salvage_tool_call(text, &advertised()).is_none());
    }

    #[test]
    fn ignores_json_naming_an_unknown_tool() {
        let text = "{\"name\": \"rm_rf\", \"arguments\": {}}";
        assert!(salvage_tool_call(text, &advertised()).is_none());
    }

    #[test]
    fn missing_arguments_defaults_to_empty_object() {
        let text = "{\"name\": \"read_file\"}";
        let call = salvage_tool_call(text, &advertised()).expect("salvaged");
        assert_eq!(call.input(), &json!({}));
    }
}
