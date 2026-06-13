use super::support::*;

#[test]
fn agent_run_calls_provider_with_read_only_tools_and_prints_final_text() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    fs::write(workspace.join("README.md"), "# codel00p\n").expect("write readme");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .header("authorization", "Bearer test-token")
            .body_includes(r#""model":"test-model""#)
            .body_includes(r#""role":"user""#)
            .body_includes(r#""content":"Inspect this project.""#)
            .body_includes(r#""name":"read_file""#)
            .body_includes(r#""name":"search_text""#)
            .body_includes(r#""name":"list_files""#);
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "The project has a README."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let output = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Inspect this project.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "The project has a README.\n");
    provider.assert();
}

#[test]
fn native_provider_agent_run_supports_openai_responses() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/responses")
            .header("authorization", "Bearer openai-token")
            .body_includes(r#""model":"gpt-5""#)
            .body_includes(r#""role":"user""#)
            .body_includes("Inspect through OpenAI Responses.")
            .body_includes(r#""name":"read_file""#)
            .body_includes(r#""name":"search_text""#)
            .body_includes(r#""name":"list_files""#);
        then.status(200).json_body(json!({
            "id": "resp_cli_native",
            "status": "completed",
            "model": "gpt-5",
            "output": [{
                "type": "message",
                "id": "msg_cli_native",
                "status": "completed",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "Responses agent ready."
                }]
            }],
            "usage": {
                "input_tokens": 12,
                "output_tokens": 4
            }
        }));
    });

    let output = run_codel00p_with_env(
        &db_path,
        &[("CODEL00P_PROVIDER_OPENAI_API_KEY", "openai-token")],
        &[
            "agent",
            "run",
            "Inspect through OpenAI Responses.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "openai",
            "--model",
            "gpt-5",
            "--base-url",
            &server.base_url(),
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Responses agent ready.\n");
    provider.assert();
}

#[test]
fn native_provider_agent_run_supports_anthropic_messages() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/messages")
            .header("x-api-key", "anthropic-token")
            .header("anthropic-version", "2023-06-01")
            .body_includes(r#""model":"claude-3-5-sonnet-latest""#)
            .body_includes(r#""role":"user""#)
            .body_includes("Inspect through Anthropic Messages.")
            .body_includes(r#""name":"read_file""#)
            .body_includes(r#""name":"search_text""#)
            .body_includes(r#""name":"list_files""#);
        then.status(200).json_body(json!({
            "id": "msg_cli_native",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-latest",
            "content": [{
                "type": "text",
                "text": "Anthropic agent ready."
            }],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 4
            }
        }));
    });

    let output = run_codel00p_with_env(
        &db_path,
        &[("CODEL00P_PROVIDER_ANTHROPIC_API_KEY", "anthropic-token")],
        &[
            "agent",
            "run",
            "Inspect through Anthropic Messages.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "anthropic",
            "--model",
            "claude-3-5-sonnet-latest",
            "--base-url",
            &server.base_url(),
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Anthropic agent ready.\n");
    provider.assert();
}

#[test]
fn native_provider_agent_run_supports_bedrock_converse() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST)
            .path("/model/test-model/converse")
            .header("content-type", "application/json")
            .header("x-amz-security-token", "bedrock-session")
            .header_matches(
                "authorization",
                "AWS4-HMAC-SHA256 Credential=bedrock-access/[0-9]{8}/us-east-1/bedrock/aws4_request, SignedHeaders=.*",
            )
            .header_matches("x-amz-date", "[0-9]{8}T[0-9]{6}Z")
            .body_includes("Inspect through Bedrock Converse.")
            .body_includes(r#""toolConfig""#)
            .body_includes(r#""name":"read_file""#)
            .body_includes(r#""name":"search_text""#)
            .body_includes(r#""name":"list_files""#);
        then.status(200).json_body(json!({
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [{"text": "Bedrock agent ready."}]
                }
            },
            "stopReason": "end_turn",
            "usage": {
                "inputTokens": 12,
                "outputTokens": 4,
                "totalTokens": 16
            }
        }));
    });

    let output = run_codel00p_with_env(
        &db_path,
        &[
            ("CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID", "bedrock-access"),
            ("CODEL00P_PROVIDER_AWS_SECRET_ACCESS_KEY", "bedrock-secret"),
            ("CODEL00P_PROVIDER_AWS_SESSION_TOKEN", "bedrock-session"),
            ("CODEL00P_PROVIDER_AWS_REGION", "us-east-1"),
        ],
        &[
            "agent",
            "run",
            "Inspect through Bedrock Converse.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "bedrock",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Bedrock agent ready.\n");
    provider.assert();
}

#[test]
fn native_provider_agent_run_supports_gemini_generate_content() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST)
            .path("/v1beta/models/gemini-2.5-flash:generateContent")
            .header("x-goog-api-key", "gemini-token")
            .body_includes(r#""role":"user""#)
            .body_includes("Inspect through Gemini GenerateContent.")
            .body_includes(r#""name":"read_file""#)
            .body_includes(r#""name":"search_text""#)
            .body_includes(r#""name":"list_files""#);
        then.status(200).json_body(json!({
            "responseId": "gemini_cli_native",
            "modelVersion": "gemini-2.5-flash",
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"text": "Gemini agent ready."}]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 12,
                "candidatesTokenCount": 4
            }
        }));
    });

    let output = run_codel00p_with_env(
        &db_path,
        &[("CODEL00P_PROVIDER_GEMINI_API_KEY", "gemini-token")],
        &[
            "agent",
            "run",
            "Inspect through Gemini GenerateContent.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "gemini",
            "--model",
            "gemini-2.5-flash",
            "--base-url",
            &server.base_url(),
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Gemini agent ready.\n");
    provider.assert();
}

#[test]
fn agent_run_defaults_to_read_only_tools() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""name":"read_file""#)
            .body_includes(r#""name":"search_text""#)
            .body_includes(r#""name":"list_files""#)
            .body_excludes(r#""name":"create_file""#)
            .body_excludes(r#""name":"run_command""#)
            .body_excludes(r#""name":"git_status""#);
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Read-only tools only."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let output = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Inspect tools.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Read-only tools only.\n");
    provider.assert();
}

#[test]
fn agent_run_can_opt_into_edit_command_and_git_tool_sets() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""name":"read_file""#)
            .body_includes(r#""name":"create_file""#)
            .body_includes(r#""name":"apply_patch""#)
            .body_includes(r#""name":"run_command""#)
            .body_includes(r#""name":"git_status""#)
            .body_includes(r#""name":"git_commit""#);
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Loaded expanded tools."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let output = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Inspect tools.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--tool-set",
            "edit",
            "--tool-set",
            "command",
            "--tool-set",
            "git",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Loaded expanded tools.\n");
    provider.assert();
}
