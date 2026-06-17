//! End-to-end MCP integration through the real `codel00p` binary.
//!
//! A local stdio MCP server fixture (a small POSIX shell script speaking the
//! MCP stdio framing: `initialize`, `notifications/initialized`, `tools/list`,
//! `tools/call`) is configured for the agent. We then drive the scripted mock
//! model to call an MCP-provided tool and assert — over the real
//! `--json-events` protocol stream and the mock's captured request bodies —
//! that the MCP tool was requested + completed, its result was fed back to the
//! model, and the run succeeded.
//!
//! Two scenarios:
//!
//! 1. **Small toolset (fully advertised).** A server exposing a single tool is
//!    folded directly into the registry; the agent calls `mcp.<server>.<tool>`
//!    straight away.
//! 2. **Large toolset (progressive disclosure).** A server exposing more than
//!    the CLI's `MCP_DISCLOSURE_THRESHOLD` (15) tools is folded in as
//!    *deferred*: the model sees `tool_search` / `tool_describe` instead of the
//!    raw MCP tool schemas, and discovers + loads the wanted tool on demand
//!    before calling it.
//!
//! # Why a shell-script fixture
//!
//! This reuses the exact fixture shape proven by the CLI integration tests in
//! `codel00p-cli/tests/agent_cli/mcp_servers.rs`: a `#!/bin/sh` script that
//! `read`s each framed request line and `printf`s a canned JSON-RPC response.
//! Stdio MCP requests arrive newline-delimited, so a sequence of blocking
//! `read`s pairs each inbound message with its reply deterministically.
//!
//! The fixture is fully hermetic (no network, no external server), and the
//! model is the standard `MockProvider`. MCP tool names are exposed to the
//! model as `mcp.<server>.<tool>`.
//!
//! Note: these tests spawn a stdio MCP subprocess with a startup timeout; per
//! repo guidance they are timing-sensitive under CPU contention, so the suite
//! must be run serially (the default for `cargo test`).

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use codel00p_e2e::{CodelRunner, MockProvider};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

/// Writes `body` as an executable `#!/bin/sh` MCP server script at `path`.
fn write_executable_script(path: &Path, body: &str) {
    std::fs::write(path, body).expect("write mcp fixture script");
    let mut perms = std::fs::metadata(path)
        .expect("stat mcp fixture script")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).expect("chmod mcp fixture script");
}

/// A stdio MCP server fixture that advertises a SINGLE `echo` tool and answers
/// one `tools/call` with `{"content":[{"type":"text","text":"echoed-e2e"}]}`.
///
/// The read/printf sequence mirrors the framing the CLI's MCP client drives:
/// `initialize` → `notifications/initialized` → `tools/list` → `tools/call`.
const SINGLE_TOOL_SERVER: &str = r#"#!/bin/sh
read init
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1.0.0"}}}'
read initialized
read list
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"echo","description":"Echo text back.","inputSchema":{"type":"object","properties":{"text":{"type":"string"}}}}]}}'
read call
printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"echoed-e2e"}],"isError":false}}'
"#;

/// Extracts the tool-result content from the Nth captured mock request body.
///
/// Request index `req_index` carries the prior round's tool results as
/// `{"role":"tool","content":"<json-or-text>"}` messages at the tail of the
/// `messages` array. We return the last tool-role message's raw `content`
/// string (MCP results are surfaced to the model as a text payload).
fn last_tool_content(provider: &MockProvider, req_index: usize) -> String {
    let requests = provider.received_requests();
    let body: Value =
        serde_json::from_str(&requests[req_index]).expect("request body should be valid JSON");
    let messages = body["messages"]
        .as_array()
        .expect("messages array in request body");
    messages
        .iter()
        .rev()
        .find(|m| m["role"] == "tool")
        .and_then(|m| m["content"].as_str())
        .expect("a tool-role message with string content")
        .to_string()
}

// ---------------------------------------------------------------------------
// 1. Small toolset — a single stdio MCP tool is fully advertised and callable.
//
// Reuses the SINGLE_TOOL_SERVER fixture (the same shape as the
// `agent_run_can_attach_stdio_mcp_servers_as_tools` CLI test) and wires it via
// the `--mcp-server <name>=<command>` flag, which the runner appends our
// provider flags after. The model calls `mcp.fake.echo`; we assert the tool
// was requested + completed, its result reached the model, and the run
// succeeded.
// ---------------------------------------------------------------------------
#[test]
fn stdio_mcp_single_tool_is_advertised_and_callable() {
    let runner = CodelRunner::new();
    let server_path = runner.workspace_path().join("single-mcp.sh");
    write_executable_script(&server_path, SINGLE_TOOL_SERVER);

    let provider = MockProvider::start()
        .tool_call("mcp.fake.echo", json!({ "text": "hello" }))
        .assistant_text("MCP echo done");

    let mcp_arg = format!("fake={}", server_path.display());
    let result = runner.with_provider(&provider).run(&[
        "agent",
        "run",
        "Use the MCP echo tool.",
        "--mcp-server",
        &mcp_arg,
    ]);

    result.assert_success();
    // The MCP tool was both requested and completed, observed over the real
    // protocol event stream.
    result.assert_tool_called("mcp.fake.echo");
    result.assert_turn_completed();

    // The MCP tool's result content was fed back to the model on the next turn.
    let tool_content = last_tool_content(&provider, 1);
    assert!(
        tool_content.contains("echoed-e2e"),
        "MCP tool result should reach the model; content: {tool_content}"
    );

    // The small toolset is fully advertised (NOT deferred): the manifest lists
    // the MCP tool itself and does not fall back to the disclosure meta-tools.
    let manifest = result.assert_context_manifest();
    if let codel00p_e2e::AgentEvent::ContextManifest {
        advertised_tools, ..
    } = manifest
    {
        assert!(
            advertised_tools.iter().any(|t| t == "mcp.fake.echo"),
            "small MCP toolset should be advertised directly; got {advertised_tools:?}"
        );
        assert!(
            !advertised_tools.iter().any(|t| t == "tool_search"),
            "small MCP toolset must NOT trigger progressive disclosure; got {advertised_tools:?}"
        );
    } else {
        panic!("expected ContextManifest event");
    }
}

// ---------------------------------------------------------------------------
// 2. Large toolset — progressive disclosure (tool_search → tool_describe →
//    call the discovered tool).
//
// A stdio server advertising 20 tools (> the CLI's threshold of 15) is folded
// in as DEFERRED. The model therefore never sees the raw MCP tool schemas; it
// sees `tool_search` / `tool_describe` instead. The scripted model:
//   turn 0: tool_search { query: "weather" }       (answered by the harness)
//   turn 1: tool_describe { names: ["mcp.big.tool_07"] }  (answered by harness)
//   turn 2: mcp.big.tool_07 { ... }                (round-trips to the server)
//   turn 3: final assistant text
//
// `tool_search` / `tool_describe` are synthetic harness tools answered from the
// in-process deferred catalog — they do NOT hit the stdio server. So the
// fixture only needs to answer one `tools/call`.
//
// We seed the server via workspace config (`.codel00p/mcp.json`), the same way
// `agent_run_loads_stdio_mcp_servers_from_workspace_config` does, since the
// runner already points `--workspace` at its own tempdir.
// ---------------------------------------------------------------------------

/// Builds a stdio MCP server script advertising `count` tools named
/// `tool_00`..`tool_<count-1>`. Tool index 7's description mentions "weather"
/// so a `tool_search` for that term ranks it. Answers exactly one `tools/call`.
fn many_tools_server(count: usize) -> String {
    let mut tools = Vec::with_capacity(count);
    for i in 0..count {
        let description = if i == 7 {
            "Report the current weather for a city.".to_string()
        } else {
            format!("Utility tool number {i}.")
        };
        tools.push(json!({
            "name": format!("tool_{i:02}"),
            "description": description,
            "inputSchema": {
                "type": "object",
                "properties": { "arg": { "type": "string" } }
            }
        }));
    }
    let tools_json = serde_json::to_string(&Value::Array(tools)).expect("serialize tools array");
    // `read call` then answer one tools/call. The harness answers tool_search /
    // tool_describe internally, so only the final real call reaches us.
    format!(
        r#"#!/bin/sh
read init
printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{{"protocolVersion":"2025-06-18","capabilities":{{"tools":{{}}}},"serverInfo":{{"name":"big","version":"1.0.0"}}}}}}'
read initialized
read list
printf '%s\n' '{{"jsonrpc":"2.0","id":2,"result":{{"tools":{tools_json}}}}}'
read call
printf '%s\n' '{{"jsonrpc":"2.0","id":3,"result":{{"content":[{{"type":"text","text":"weather-report-e2e"}}],"isError":false}}}}'
"#
    )
}

#[test]
fn stdio_mcp_large_toolset_is_deferred_and_discovered_via_progressive_disclosure() {
    // 20 tools is comfortably over the CLI's MCP_DISCLOSURE_THRESHOLD (15).
    // A relative `command` in mcp.json is resolved against the WORKSPACE root
    // (see `load_mcp_servers_from_workspace`), so the script lives at the
    // workspace root while the config itself lives under `.codel00p/`.
    let runner = CodelRunner::new()
        .workspace_file("many-mcp.sh", many_tools_server(20))
        .workspace_file(
            ".codel00p/mcp.json",
            json!({
                "servers": {
                    "big": { "command": "./many-mcp.sh", "timeoutMs": 5000 }
                }
            })
            .to_string(),
        );
    let server_path = runner.workspace_path().join("many-mcp.sh");
    let mut perms = std::fs::metadata(&server_path)
        .expect("stat many-mcp.sh")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&server_path, perms).expect("chmod many-mcp.sh");

    let provider = MockProvider::start()
        // Discover: search the deferred catalog for the weather tool.
        .tool_call("tool_search", json!({ "query": "weather" }))
        // Load its schema by name.
        .tool_call("tool_describe", json!({ "names": ["mcp.big.tool_07"] }))
        // Call the now-discovered MCP tool.
        .tool_call("mcp.big.tool_07", json!({ "arg": "London" }))
        .assistant_text("weather fetched via progressive disclosure");

    let result =
        runner
            .with_provider(&provider)
            .run(&["agent", "run", "Find and use the weather tool."]);

    result.assert_success();
    result.assert_turn_completed();

    // The full progressive-disclosure path was exercised end-to-end.
    result.assert_tool_called("tool_search");
    result.assert_tool_called("tool_describe");
    result.assert_tool_called("mcp.big.tool_07");

    // The deferred MCP tools are NOT advertised up front; the synthetic
    // disclosure tools are advertised in their place.
    let manifest = result.assert_context_manifest();
    if let codel00p_e2e::AgentEvent::ContextManifest {
        advertised_tools, ..
    } = manifest
    {
        assert!(
            advertised_tools.iter().any(|t| t == "tool_search"),
            "large MCP toolset should advertise tool_search; got {advertised_tools:?}"
        );
        assert!(
            advertised_tools.iter().any(|t| t == "tool_describe"),
            "large MCP toolset should advertise tool_describe; got {advertised_tools:?}"
        );
        assert!(
            !advertised_tools.iter().any(|t| t == "mcp.big.tool_07"),
            "deferred MCP tools must be withheld from the manifest; got {advertised_tools:?}"
        );
    } else {
        panic!("expected ContextManifest event");
    }

    // `tool_search` returned a hit set drawn from the deferred catalog: the
    // result fed back on the next turn (request index 1) names the weather
    // tool and reports the deferred total.
    let search_result = last_tool_content(&provider, 1);
    let search_json: Value =
        serde_json::from_str(&search_result).expect("tool_search result is JSON");
    assert_eq!(
        search_json["deferred_total"], 20,
        "all 20 MCP tools should be deferred; result: {search_json}"
    );
    assert!(
        search_json["tools"]
            .as_array()
            .map(|tools| tools.iter().any(|t| t["name"] == "mcp.big.tool_07"))
            .unwrap_or(false),
        "tool_search for 'weather' should surface mcp.big.tool_07; result: {search_json}"
    );

    // `tool_describe` returned the schema for the named tool (request index 2).
    let describe_result = last_tool_content(&provider, 2);
    let describe_json: Value =
        serde_json::from_str(&describe_result).expect("tool_describe result is JSON");
    let described = describe_json["tools"]
        .as_array()
        .expect("describe returns a tools array");
    assert!(
        described
            .iter()
            .any(|t| t["name"] == "mcp.big.tool_07" && t.get("input_schema").is_some()),
        "tool_describe should return mcp.big.tool_07's schema; result: {describe_json}"
    );

    // The discovered MCP tool's real result round-tripped back to the model
    // (request index 3).
    let call_result = last_tool_content(&provider, 3);
    assert!(
        call_result.contains("weather-report-e2e"),
        "discovered MCP tool result should reach the model; content: {call_result}"
    );
}
