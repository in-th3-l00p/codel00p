//! End-to-end check that a `PluginRegistry` applied to an `AgentHarness`
//! actually contributes a tool and a lifecycle hook to a real turn.

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use async_trait::async_trait;
use codel00p_harness::{
    AgentHarness, HarnessError, HarnessInferenceRequest, HarnessInferenceResponse, LifecycleHook,
    ModelClient, ModelToolCall, SessionId, Tool, ToolRegistry, ToolResult, TurnLifecycleContext,
    UserMessage, Workspace,
};
use codel00p_plugin::{Plugin, PluginRegistry};
use serde_json::{Value, json};

/// Returns scripted responses in order, so we can drive the agent loop without
/// a real provider.
struct ScriptedModel {
    responses: Mutex<VecDeque<HarnessInferenceResponse>>,
}

#[async_trait]
impl ModelClient for ScriptedModel {
    async fn infer(
        &self,
        _request: HarnessInferenceRequest,
    ) -> Result<HarnessInferenceResponse, HarnessError> {
        Ok(self
            .responses
            .lock()
            .expect("model lock")
            .pop_front()
            .expect("a scripted response"))
    }
}

/// A tool that echoes a fixed marker, so the test can confirm the plugin's tool
/// (not some default) is what executed.
struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "echo a marker"
    }

    fn input_schema(&self) -> Value {
        json!({ "type": "object" })
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        _input: Value,
    ) -> Result<ToolResult, HarnessError> {
        Ok(ToolResult::json(json!({ "marker": "wired" })))
    }
}

/// Records that the turn-started hook fired.
struct RecordingHook {
    turns: Arc<AtomicUsize>,
}

#[async_trait]
impl LifecycleHook for RecordingHook {
    async fn on_turn_started(&self, _context: TurnLifecycleContext) -> Result<(), HarnessError> {
        self.turns.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct WiringPlugin {
    turns: Arc<AtomicUsize>,
}

impl Plugin for WiringPlugin {
    fn name(&self) -> &str {
        "wiring"
    }

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        vec![Arc::new(EchoTool)]
    }

    fn lifecycle_hooks(&self) -> Vec<Arc<dyn LifecycleHook>> {
        vec![Arc::new(RecordingHook {
            turns: self.turns.clone(),
        })]
    }
}

#[tokio::test]
async fn plugin_tool_and_hook_drive_a_real_turn() {
    let turns = Arc::new(AtomicUsize::new(0));
    let plugins = PluginRegistry::new().register(Arc::new(WiringPlugin {
        turns: turns.clone(),
    }));

    // First the model asks for the plugin tool, then it produces final text.
    let model = ScriptedModel {
        responses: Mutex::new(VecDeque::from(vec![
            HarnessInferenceResponse::with_tool_calls(
                "test",
                "test-model",
                vec![ModelToolCall::new("call-1", "echo", json!({}))],
            ),
            HarnessInferenceResponse::assistant("test", "test-model", "done"),
        ])),
    };

    let workspace = Workspace::new(std::env::temp_dir()).expect("workspace");
    let tools = plugins.apply_to_tool_registry(ToolRegistry::new());

    let builder = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(tools)
        .max_iterations(4);

    let outcome = plugins
        .apply_to_harness_builder(builder)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("plugin-wiring"),
            UserMessage::new("hi"),
        )
        .await
        .expect("run turn");

    assert_eq!(outcome.assistant_message.as_deref(), Some("done"));

    // The plugin-contributed tool executed and returned its marker.
    assert_eq!(outcome.tool_calls.len(), 1);
    assert_eq!(outcome.tool_calls[0].name, "echo");
    assert_eq!(
        outcome.tool_calls[0].result.content(),
        &json!({ "marker": "wired" })
    );

    // The plugin-contributed lifecycle hook fired exactly once.
    assert_eq!(turns.load(Ordering::SeqCst), 1);
}
