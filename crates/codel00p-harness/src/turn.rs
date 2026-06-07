use async_trait::async_trait;
pub use codel00p_protocol::ToolCall as ModelToolCall;
use serde::{Deserialize, Serialize};

use crate::{
    errors::HarnessError, events::HarnessEvent, session::SessionState, tool_result::ToolResult,
};

#[async_trait]
pub trait ModelClient: Send + Sync {
    async fn infer(
        &self,
        request: HarnessInferenceRequest,
    ) -> Result<HarnessInferenceResponse, HarnessError>;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HarnessInferenceRequest {
    session_state: SessionState,
    workspace_root: Option<String>,
    tool_names: Vec<String>,
}

impl HarnessInferenceRequest {
    pub fn new(session_state: SessionState) -> Self {
        Self {
            session_state,
            workspace_root: None,
            tool_names: Vec::new(),
        }
    }

    pub fn with_runtime_context(
        mut self,
        workspace_root: impl Into<String>,
        tool_names: Vec<String>,
    ) -> Self {
        self.workspace_root = Some(workspace_root.into());
        self.tool_names = tool_names;
        self
    }

    pub fn session_state(&self) -> &SessionState {
        &self.session_state
    }

    pub fn workspace_root(&self) -> Option<&str> {
        self.workspace_root.as_deref()
    }

    pub fn tool_names(&self) -> &[String] {
        &self.tool_names
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HarnessInferenceResponse {
    provider: String,
    model: String,
    assistant_message: Option<String>,
    tool_calls: Vec<ModelToolCall>,
    finish_reason: Option<String>,
}

impl HarnessInferenceResponse {
    pub fn assistant(
        provider: impl Into<String>,
        model: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            assistant_message: Some(content.into()),
            tool_calls: Vec::new(),
            finish_reason: Some("stop".to_string()),
        }
    }

    pub fn with_tool_calls(
        provider: impl Into<String>,
        model: impl Into<String>,
        tool_calls: Vec<ModelToolCall>,
    ) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            assistant_message: None,
            tool_calls,
            finish_reason: Some("tool_calls".to_string()),
        }
    }

    pub fn from_parts(
        provider: impl Into<String>,
        model: impl Into<String>,
        assistant_message: Option<String>,
        tool_calls: Vec<ModelToolCall>,
        finish_reason: Option<String>,
    ) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            assistant_message,
            tool_calls,
            finish_reason,
        }
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn assistant_message(&self) -> Option<&str> {
        self.assistant_message.as_deref()
    }

    pub fn tool_calls(&self) -> &[ModelToolCall] {
        &self.tool_calls
    }

    pub fn finish_reason(&self) -> Option<&str> {
        self.finish_reason.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecutedToolCall {
    pub id: String,
    pub name: String,
    pub result: ToolResult,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnOutcome {
    pub assistant_message: Option<String>,
    pub tool_calls: Vec<ExecutedToolCall>,
    pub events: Vec<HarnessEvent>,
    pub session_state: SessionState,
}
