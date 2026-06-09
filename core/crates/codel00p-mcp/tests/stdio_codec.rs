use codel00p_mcp::{
    JsonRpcMessage, JsonRpcNotification, JsonRpcRequest, decode_stdio_message, encode_stdio_message,
};
use serde_json::json;

#[test]
fn encodes_json_rpc_messages_as_single_newline_delimited_utf8_lines() {
    let message = JsonRpcMessage::request(JsonRpcRequest::new(
        7,
        "tools/call",
        json!({
            "name": "search",
            "arguments": { "query": "memory" }
        }),
    ));

    let encoded = encode_stdio_message(&message).expect("encode message");

    assert!(encoded.ends_with('\n'));
    assert_eq!(encoded.lines().count(), 1);
    assert!(encoded.contains(r#""jsonrpc":"2.0""#));
    assert!(encoded.contains(r#""method":"tools/call""#));
}

#[test]
fn decodes_single_line_json_rpc_messages() {
    let decoded =
        decode_stdio_message(r#"{"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}}"#)
            .expect("decode message");

    assert_eq!(
        decoded,
        JsonRpcMessage::request(JsonRpcRequest::new(3, "tools/list", json!({})))
    );
}

#[test]
fn decodes_json_rpc_notifications_without_ids() {
    let decoded = decode_stdio_message(
        r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#,
    )
    .expect("decode notification");

    assert_eq!(
        decoded,
        JsonRpcMessage::Notification(JsonRpcNotification::new(
            "notifications/initialized",
            json!({})
        ))
    );
}

#[test]
fn rejects_embedded_newlines_in_stdio_messages() {
    let error = decode_stdio_message("{\"jsonrpc\":\"2.0\"}\n{\"jsonrpc\":\"2.0\"}")
        .expect_err("embedded newline should be rejected");

    assert!(error.to_string().contains("embedded newline"));
}
