//! End-to-end tests for capability synthesis.
//!
//! `harness_runs_a_capability_in_one_turn` is offline and deterministic (a
//! scripted model) and proves the builder wiring: a registered capability is
//! advertised and, when called, runs its frozen governed pipeline.
//!
//! `live_model_invokes_a_synthesized_capability` is gated on an OpenRouter key
//! (`CODEL00P_PROVIDER_OPENROUTER_API_KEY` or `OPENROUTER_API_KEY`) and proves
//! the payoff against a real model: the model discovers the synthesized
//! capability and accomplishes a multi-file scaffold in a single tool call.

mod support;

use codel00p_harness::{
    AgentHarness, Capability, HarnessInferenceResponse, ModelToolCall, SessionId, ToolRegistry,
    UserMessage, Workspace,
};
use serde_json::json;
use support::ScriptedModelClient;
use tempfile::tempdir;

/// The capability under test: scaffold a module's source + test files.
fn scaffold_capability() -> Capability {
    serde_json::from_value(json!({
        "name": "scaffold_module",
        "description": "Create a Rust source file and its test file for a module named `name`.",
        "parameters": {
            "type": "object",
            "required": ["name"],
            "properties": { "name": { "type": "string" } }
        },
        "steps": [
            {
                "tool": "create_file",
                "input": {
                    "path": "src/{{params.name}}.rs",
                    "content": "//! The {{params.name}} module.\npub fn {{params.name}}() {}\n"
                }
            },
            {
                "tool": "create_file",
                "input": {
                    "path": "tests/{{params.name}}_test.rs",
                    "content": "// tests for {{params.name}}\n"
                }
            }
        ]
    }))
    .unwrap()
}

#[tokio::test]
async fn harness_runs_a_capability_in_one_turn() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-1",
                "scaffold_module",
                json!({ "name": "widget" }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Scaffolded."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(ToolRegistry::editing_defaults())
        .capabilities(vec![scaffold_capability()])
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-capability"),
            UserMessage::new("Scaffold the widget module."),
        )
        .await
        .expect("run turn");

    // The capability was advertised to the model...
    assert!(
        model.requests()[0]
            .tool_names()
            .contains(&"scaffold_module".to_string())
    );
    // ...and the single capability call expanded into the whole frozen pipeline.
    assert_eq!(outcome.tool_calls.len(), 1);
    assert_eq!(outcome.tool_calls[0].name, "scaffold_module");
    let content = outcome.tool_calls[0].result.content();
    assert_eq!(content["ok"], true);
    assert_eq!(content["completed"], 2);

    assert!(dir.path().join("src/widget.rs").exists());
    assert!(dir.path().join("tests/widget_test.rs").exists());
    let body = std::fs::read_to_string(dir.path().join("src/widget.rs")).unwrap();
    assert!(body.contains("pub fn widget()"));
}

/// Read the OpenRouter API key from the same env vars the provider registry uses.
fn openrouter_key() -> Option<String> {
    std::env::var("CODEL00P_PROVIDER_OPENROUTER_API_KEY")
        .or_else(|_| std::env::var("OPENROUTER_API_KEY"))
        .ok()
        .filter(|key| !key.trim().is_empty())
}

#[tokio::test]
async fn live_model_invokes_a_synthesized_capability() {
    let Some(key) = openrouter_key() else {
        eprintln!(
            "skipping live capability test: set CODEL00P_PROVIDER_OPENROUTER_API_KEY to run it"
        );
        return;
    };
    let model_id =
        std::env::var("CODEL00P_E2E_MODEL").unwrap_or_else(|_| "openai/gpt-4o-mini".to_string());

    use codel00p_harness::ProviderModelClient;
    use codel00p_providers::{Credential, InferenceClient, default_registry};

    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    // Advertise ONLY the synthesized capability, so a single obvious tool call
    // accomplishes the multi-file scaffold. The capability's engine still runs
    // the real `create_file` steps under the hood.
    let tools = codel00p_harness::capability_tools(
        ToolRegistry::editing_defaults(),
        std::sync::Arc::new(codel00p_harness::AllowAllPermissionPolicy),
        vec![scaffold_capability()],
    )
    .expect("capability tools");

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openrouter", Credential::api_key(key))
        .build();
    let model_client = ProviderModelClient::new(client, "openrouter", &model_id)
        .with_base_url("https://openrouter.ai/api/v1");

    let outcome = AgentHarness::builder()
        .model_client(model_client)
        .workspace(workspace)
        .tools(tools)
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-live-capability"),
            UserMessage::new(
                "Scaffold a new module named `widget` in this Rust project by calling the \
                 available tool. After it succeeds, reply with the single word done.",
            ),
        )
        .await
        .expect("run turn");

    // The real model discovered and called the synthesized capability...
    assert!(
        outcome
            .tool_calls
            .iter()
            .any(|call| call.name == "scaffold_module"),
        "expected the model to call scaffold_module; calls: {:?}",
        outcome
            .tool_calls
            .iter()
            .map(|c| &c.name)
            .collect::<Vec<_>>()
    );
    // ...and the frozen pipeline produced both files.
    assert!(
        dir.path().join("src/widget.rs").exists(),
        "src/widget.rs was not created"
    );
    assert!(
        dir.path().join("tests/widget_test.rs").exists(),
        "tests/widget_test.rs was not created"
    );
}
