//! Bridging values between Rhai (`Dynamic`) and JSON (`serde_json::Value`), plus
//! the result-size cap. Pure conversions with no engine or I/O — the bidirectional
//! mapping the code-execution tool relies on, isolated so it is easy to read and
//! test on its own.

use super::*;

/// Cap the JSON-encoded size of the script's return value. Over-budget values are
/// replaced by a string preview so the model still sees the head of the output
/// without flooding context.
pub(super) fn cap_result(value: Value) -> (Value, bool) {
    let encoded = value.to_string();
    if encoded.len() <= MAX_RESULT_BYTES {
        return (value, false);
    }
    let mut preview: String = encoded.chars().take(MAX_RESULT_BYTES).collect();
    preview.push_str("…[truncated]");
    (Value::String(preview), true)
}

/// Convert a Rhai [`Dynamic`] into a [`serde_json::Value`].
///
/// Rhai's `serde` feature could do this, but a direct conversion keeps the
/// dependency surface minimal and handles the small set of types scripts return
/// (numbers, strings, bools, arrays, maps, unit). Unsupported exotic types
/// stringify, which is safe for a result payload.
pub(crate) fn dynamic_to_json(value: Dynamic) -> Value {
    if value.is_unit() {
        return Value::Null;
    }
    if value.is_bool() {
        return Value::Bool(value.as_bool().unwrap_or(false));
    }
    if value.is_int() {
        return json!(value.as_int().unwrap_or(0));
    }
    if value.is_float() {
        return serde_json::Number::from_f64(value.as_float().unwrap_or(0.0))
            .map(Value::Number)
            .unwrap_or(Value::Null);
    }
    if value.is_string() {
        return Value::String(value.into_string().unwrap_or_default());
    }
    if value.is_array() {
        let array = value.cast::<rhai::Array>();
        return Value::Array(array.into_iter().map(dynamic_to_json).collect());
    }
    if value.is_map() {
        let map = value.cast::<RhaiMap>();
        let object = map
            .into_iter()
            .map(|(k, v)| (k.to_string(), dynamic_to_json(v)))
            .collect();
        return Value::Object(object);
    }
    // Fallback: stringify anything else (e.g. char, timestamp).
    Value::String(value.to_string())
}

/// Convert a [`serde_json::Value`] into a Rhai [`Dynamic`] so a tool result can
/// be handed back to the script as a native map/array/scalar.
pub(crate) fn json_to_dynamic(value: &Value) -> Dynamic {
    match value {
        Value::Null => Dynamic::UNIT,
        Value::Bool(b) => Dynamic::from(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Dynamic::from(i)
            } else if let Some(f) = n.as_f64() {
                Dynamic::from(f)
            } else {
                Dynamic::UNIT
            }
        }
        Value::String(s) => Dynamic::from(s.clone()),
        Value::Array(items) => {
            let array: rhai::Array = items.iter().map(json_to_dynamic).collect();
            Dynamic::from(array)
        }
        Value::Object(map) => {
            let mut rmap = RhaiMap::new();
            for (k, v) in map {
                rmap.insert(k.as_str().into(), json_to_dynamic(v));
            }
            Dynamic::from(rmap)
        }
    }
}
