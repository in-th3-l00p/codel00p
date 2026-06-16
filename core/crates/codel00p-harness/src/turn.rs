use async_trait::async_trait;
use codel00p_protocol::ContextWindowState;
pub use codel00p_protocol::ToolCall as ModelToolCall;
pub use codel00p_providers::{ResponseFormat, TokenSink, ToolChoice};
use serde::{Deserialize, Serialize};

use crate::{
    errors::HarnessError, events::HarnessEvent, instructions::ProjectInstructions,
    memory::ProjectMemoryContext, session::SessionState, skills::SkillContext,
    tool_result::ToolResult, tools::ToolSpec,
};

#[async_trait]
pub trait ModelClient: Send + Sync {
    async fn infer(
        &self,
        request: HarnessInferenceRequest,
    ) -> Result<HarnessInferenceResponse, HarnessError>;

    /// Runs inference while streaming assistant text to `sink` as it arrives.
    ///
    /// The default implementation performs a normal [`ModelClient::infer`] call
    /// and emits the whole assistant message to `sink` once, so clients that do
    /// not implement real streaming still drive a token sink correctly.
    async fn infer_streaming(
        &self,
        request: HarnessInferenceRequest,
        sink: &dyn TokenSink,
    ) -> Result<HarnessInferenceResponse, HarnessError> {
        let response = self.infer(request).await?;
        if let Some(message) = response.assistant_message() {
            sink.on_token(message);
        }
        Ok(response)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HarnessInferenceRequest {
    session_state: SessionState,
    workspace_root: Option<String>,
    /// Tool names, kept in sync with `tools` so `tool_names()` stays a cheap
    /// borrowed slice for events and callers that only need identifiers.
    tool_names: Vec<String>,
    /// Full model-facing tool definitions (name, description, JSON Schema).
    tools: Vec<ToolSpec>,
    /// Optional control over whether/which tool the model must call.
    tool_choice: Option<ToolChoice>,
    /// Optional structured-output request (JSON mode).
    response_format: Option<ResponseFormat>,
    context_window: Option<ContextWindowState>,
    project_instructions: Option<ProjectInstructions>,
    project_memory: Option<ProjectMemoryContext>,
    skills: Option<SkillContext>,
}

impl HarnessInferenceRequest {
    pub fn new(session_state: SessionState) -> Self {
        Self {
            session_state,
            workspace_root: None,
            tool_names: Vec::new(),
            tools: Vec::new(),
            tool_choice: None,
            response_format: None,
            context_window: None,
            project_instructions: None,
            project_memory: None,
            skills: None,
        }
    }

    pub fn with_runtime_context(
        mut self,
        workspace_root: impl Into<String>,
        tools: Vec<ToolSpec>,
    ) -> Self {
        self.workspace_root = Some(workspace_root.into());
        self.tool_names = tools.iter().map(|tool| tool.name.clone()).collect();
        self.tools = tools;
        self
    }

    pub fn with_context_window(mut self, context_window: ContextWindowState) -> Self {
        self.context_window = Some(context_window);
        self
    }

    pub fn with_project_instructions(mut self, project_instructions: ProjectInstructions) -> Self {
        self.project_instructions = Some(project_instructions);
        self
    }

    pub fn with_project_memory(mut self, project_memory: ProjectMemoryContext) -> Self {
        self.project_memory = Some(project_memory);
        self
    }

    pub fn with_skills(mut self, skills: SkillContext) -> Self {
        self.skills = Some(skills);
        self
    }

    pub fn with_tool_choice(mut self, tool_choice: ToolChoice) -> Self {
        self.tool_choice = Some(tool_choice);
        self
    }

    pub fn with_response_format(mut self, response_format: ResponseFormat) -> Self {
        self.response_format = Some(response_format);
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

    /// The full model-facing tool definitions advertised to the provider.
    pub fn tools(&self) -> &[ToolSpec] {
        &self.tools
    }

    /// The configured tool-choice control, if any.
    pub fn tool_choice(&self) -> Option<&ToolChoice> {
        self.tool_choice.as_ref()
    }

    /// The configured structured-output format, if any.
    pub fn response_format(&self) -> Option<&ResponseFormat> {
        self.response_format.as_ref()
    }

    pub fn context_window(&self) -> Option<&ContextWindowState> {
        self.context_window.as_ref()
    }

    pub fn project_instructions(&self) -> Option<&ProjectInstructions> {
        self.project_instructions.as_ref()
    }

    pub fn project_memory(&self) -> Option<&ProjectMemoryContext> {
        self.project_memory.as_ref()
    }

    pub fn skills(&self) -> Option<&SkillContext> {
        self.skills.as_ref()
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
    /// Set when the turn stopped early because cancellation was requested. The
    /// accumulated messages and tool results are still valid and persistable.
    #[serde(default)]
    pub cancelled: bool,
}
