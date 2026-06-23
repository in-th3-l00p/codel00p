//! Best-effort repair of common LLM tool-argument malformations.
//!
//! Smaller models routinely emit *almost*-valid tool calls: the whole arguments
//! object double-encoded as a JSON string, a single value where the schema wants
//! a one-element array, a number or boolean sent as a string. Each of those would
//! otherwise fail validation and burn a model round-trip to fix.
//!
//! [`coerce_tool_input`] runs just before validation in the registry and repairs
//! these against the tool's JSON Schema, so a coercible mistake succeeds the first
//! time. It is conservative: only well-understood shapes are touched, anything
//! already valid passes through unchanged, and a value that cannot be safely
//! coerced is left as-is for validation to reject (with the schema echoed back).

use serde_json::Value;

/// Repair `input` toward `schema`. Returns the (possibly unchanged) value to
/// validate and execute with.
pub(crate) fn coerce_tool_input(schema: &Value, input: Value) -> Value {
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        return input;
    }

    // Whole-arguments-as-a-JSON-string: some providers/models double-encode the
    // arguments object. Decode it once if it yields an object.
    let input = decode_json_object_string(input);

    let Value::Object(mut map) = input else {
        return input;
    };
    let Some(Value::Object(properties)) = schema.get("properties") else {
        return Value::Object(map);
    };

    for (field, value) in map.iter_mut() {
        let Some(expected) = properties
            .get(field)
            .and_then(|spec| spec.get("type"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        coerce_field(expected, value);
    }
    Value::Object(map)
}

/// If `value` is a JSON-object string (e.g. `"{\"path\":\"a\"}"`), decode it to
/// the object. Leaves every other value untouched.
fn decode_json_object_string(value: Value) -> Value {
    if let Value::String(text) = &value
        && let Ok(decoded @ Value::Object(_)) = serde_json::from_str::<Value>(text)
    {
        return decoded;
    }
    value
}

/// Coerce a single field value toward its declared `expected` JSON Schema type,
/// in place. Only safe, unambiguous repairs are applied.
fn coerce_field(expected: &str, value: &mut Value) {
    match expected {
        // A lone scalar/object where an array is wanted: wrap it. A null is left
        // for validation (an explicit "no value", not a one-element list).
        "array" if !value.is_array() && !value.is_null() => {
            *value = Value::Array(vec![value.take()]);
        }
        // A JSON-object string where an object is wanted: decode it.
        "object" if value.is_string() => {
            let decoded = decode_json_object_string(value.take());
            *value = decoded;
        }
        // Numbers sent as strings ("3", "3.5").
        "integer" | "number" => {
            if let Some(text) = value.as_str()
                && let Ok(parsed) = serde_json::from_str::<Value>(text.trim())
                && parsed.is_number()
            {
                *value = parsed;
            }
        }
        // Booleans sent as strings ("true"/"false"), case-insensitively.
        "boolean" => {
            if let Some(text) = value.as_str() {
                match text.trim().to_ascii_lowercase().as_str() {
                    "true" => *value = Value::Bool(true),
                    "false" => *value = Value::Bool(false),
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "limit": { "type": "integer" },
                "ratio": { "type": "number" },
                "force": { "type": "boolean" },
                "tags": { "type": "array" },
                "opts": { "type": "object" }
            }
        })
    }

    #[test]
    fn decodes_whole_arguments_json_string() {
        let input = json!("{\"path\": \"a.rs\", \"limit\": 3}");
        let out = coerce_tool_input(&schema(), input);
        assert_eq!(out, json!({ "path": "a.rs", "limit": 3 }));
    }

    #[test]
    fn wraps_scalar_into_array_for_array_field() {
        let out = coerce_tool_input(&schema(), json!({ "tags": "urgent" }));
        assert_eq!(out["tags"], json!(["urgent"]));
    }

    #[test]
    fn parses_numeric_and_boolean_strings() {
        let out = coerce_tool_input(
            &schema(),
            json!({ "limit": "5", "ratio": "0.25", "force": "TRUE" }),
        );
        assert_eq!(out["limit"], json!(5));
        assert_eq!(out["ratio"], json!(0.25));
        assert_eq!(out["force"], json!(true));
    }

    #[test]
    fn decodes_object_string_field() {
        let out = coerce_tool_input(&schema(), json!({ "opts": "{\"deep\": true}" }));
        assert_eq!(out["opts"], json!({ "deep": true }));
    }

    #[test]
    fn leaves_valid_input_untouched() {
        let valid = json!({ "path": "a.rs", "limit": 3, "tags": ["x"], "force": true });
        assert_eq!(coerce_tool_input(&schema(), valid.clone()), valid);
    }

    #[test]
    fn leaves_uncoercible_values_for_validation() {
        // A non-numeric string for an integer field is not repaired (validation
        // will reject it with the schema echoed back).
        let out = coerce_tool_input(&schema(), json!({ "limit": "soon" }));
        assert_eq!(out["limit"], json!("soon"));
        // An already-array value for an array field is not double-wrapped.
        let out = coerce_tool_input(&schema(), json!({ "tags": ["a", "b"] }));
        assert_eq!(out["tags"], json!(["a", "b"]));
        // A null where an array is wanted is left alone (explicit absence).
        let out = coerce_tool_input(&schema(), json!({ "tags": null }));
        assert_eq!(out["tags"], json!(null));
    }

    #[test]
    fn non_object_schema_is_passthrough() {
        let any = json!({ "type": "string" });
        assert_eq!(coerce_tool_input(&any, json!("hello")), json!("hello"));
    }

    #[test]
    fn non_object_input_that_is_not_a_json_string_passes_through() {
        // A bare array stays a bare array (validation reports the shape error).
        assert_eq!(coerce_tool_input(&schema(), json!([1, 2])), json!([1, 2]));
    }
}
