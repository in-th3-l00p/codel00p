//! Validation of tool-call arguments against a tool's JSON Schema.
//!
//! The model can emit malformed tool calls (missing fields, wrong types). Rather
//! than let those reach a tool's `execute` and fail with an ad-hoc message — or
//! worse, after a side effect — the registry validates arguments first and
//! returns a structured, model-readable [`HarnessError::InvalidToolInput`]. The
//! turn loop feeds that back as a tool result, so the model self-corrects on the
//! next step (the Hermes pattern).
//!
//! This is a deliberately small JSON Schema subset — object `type`, `required`,
//! and declared property `type`s — which is what the built-in and MCP tool
//! schemas use. It is lenient: anything it does not understand passes, so a
//! richer schema never causes a false rejection.

use serde_json::Value;

use crate::errors::HarnessError;

/// Validates `input` against `schema` for tool `tool`. `Ok(())` when the schema
/// is not an object schema or the input conforms.
pub(crate) fn validate_tool_input(
    tool: &str,
    schema: &Value,
    input: &Value,
) -> Result<(), HarnessError> {
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        return Ok(());
    }

    let Some(input_obj) = input.as_object() else {
        return Err(invalid(
            tool,
            format!(
                "expected a JSON object of arguments, got {}",
                json_type(input)
            ),
        ));
    };

    let mut problems = Vec::new();

    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for field in required.iter().filter_map(Value::as_str) {
            match input_obj.get(field) {
                None | Some(Value::Null) => {
                    problems.push(format!("missing required field `{field}`"))
                }
                _ => {}
            }
        }
    }

    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (field, value) in input_obj {
            // Unknown/extra fields are tolerated; only declared types are checked.
            if let Some(expected) = properties
                .get(field)
                .and_then(|spec| spec.get("type"))
                .and_then(Value::as_str)
                && !matches_type(expected, value)
            {
                problems.push(format!(
                    "field `{field}` must be {expected}, got {}",
                    json_type(value)
                ));
            }
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(invalid(
            tool,
            format!("invalid arguments: {}", problems.join("; ")),
        ))
    }
}

fn invalid(tool: &str, message: String) -> HarnessError {
    HarnessError::InvalidToolInput {
        name: tool.to_string(),
        message,
    }
}

fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn matches_type(expected: &str, value: &Value) -> bool {
    match expected {
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "null" => value.is_null(),
        // Unknown type keyword: do not reject.
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "type": "string" },
                "limit": { "type": "integer" }
            }
        })
    }

    #[test]
    fn accepts_valid_input() {
        assert!(validate_tool_input("read_file", &schema(), &json!({ "path": "a.rs" })).is_ok());
        assert!(
            validate_tool_input(
                "read_file",
                &schema(),
                &json!({ "path": "a.rs", "limit": 10 })
            )
            .is_ok()
        );
        // Extra undeclared fields are tolerated.
        assert!(
            validate_tool_input(
                "read_file",
                &schema(),
                &json!({ "path": "a.rs", "extra": 1 })
            )
            .is_ok()
        );
    }

    #[test]
    fn rejects_missing_required_field() {
        let error =
            validate_tool_input("read_file", &schema(), &json!({ "limit": 1 })).unwrap_err();
        let HarnessError::InvalidToolInput { name, message } = error else {
            panic!("expected InvalidToolInput");
        };
        assert_eq!(name, "read_file");
        assert!(message.contains("missing required field `path`"));
    }

    #[test]
    fn rejects_wrong_property_type() {
        let error = validate_tool_input(
            "read_file",
            &schema(),
            &json!({ "path": "a.rs", "limit": "ten" }),
        )
        .unwrap_err();
        let HarnessError::InvalidToolInput { message, .. } = error else {
            panic!("expected InvalidToolInput");
        };
        assert!(message.contains("field `limit` must be integer, got string"));
    }

    #[test]
    fn rejects_non_object_arguments() {
        let error = validate_tool_input("read_file", &schema(), &json!("nope")).unwrap_err();
        let HarnessError::InvalidToolInput { message, .. } = error else {
            panic!("expected InvalidToolInput");
        };
        assert!(message.contains("expected a JSON object"));
    }

    #[test]
    fn lenient_when_schema_is_not_an_object() {
        // No object type → no validation.
        assert!(validate_tool_input("x", &json!({}), &json!("anything")).is_ok());
        assert!(validate_tool_input("x", &json!({ "type": "string" }), &json!(5)).is_ok());
    }

    #[test]
    fn integer_accepts_only_integers_number_accepts_floats() {
        let s = json!({ "type": "object", "properties": { "n": { "type": "integer" } } });
        assert!(validate_tool_input("x", &s, &json!({ "n": 3 })).is_ok());
        assert!(validate_tool_input("x", &s, &json!({ "n": 3.5 })).is_err());

        let s = json!({ "type": "object", "properties": { "n": { "type": "number" } } });
        assert!(validate_tool_input("x", &s, &json!({ "n": 3.5 })).is_ok());
    }
}
