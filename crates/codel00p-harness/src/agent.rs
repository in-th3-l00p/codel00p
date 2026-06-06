use std::sync::Arc;

use crate::{
    errors::HarnessError,
    events::HarnessEvent,
    session::{SessionId, SessionState, TurnId, UserMessage},
    tool_registry::ToolRegistry,
    turn::{HarnessInferenceRequest, ModelClient, TurnOutcome},
    workspace::Workspace,
};

pub struct AgentHarness {
    model_client: Arc<dyn ModelClient>,
    workspace: Workspace,
    tools: ToolRegistry,
    max_iterations: u32,
}

impl AgentHarness {
    pub fn builder() -> AgentHarnessBuilder {
        AgentHarnessBuilder::default()
    }

    pub async fn run_turn(
        self,
        session_id: SessionId,
        user_message: UserMessage,
    ) -> Result<TurnOutcome, HarnessError> {
        let turn_id = TurnId::new();
        let mut events = vec![HarnessEvent::TurnStarted {
            session_id: session_id.clone(),
            turn_id,
        }];
        let mut session_state = SessionState::new(session_id);
        session_state.push_user(user_message);
        events.push(HarnessEvent::ContextBuilt {
            message_count: session_state.messages().len(),
        });

        let response = self
            .model_client
            .infer(
                HarnessInferenceRequest::new(session_state.clone()).with_runtime_context(
                    self.workspace.root().display().to_string(),
                    self.tools.names().into_iter().map(str::to_string).collect(),
                ),
            )
            .await?;

        events.push(HarnessEvent::InferenceRequested {
            provider: response.provider().to_string(),
            model: response.model().to_string(),
        });
        events.push(HarnessEvent::InferenceCompleted {
            finish_reason: response.finish_reason().map(str::to_string),
        });

        let assistant_message = response.assistant_message().map(str::to_string);
        if let Some(content) = &assistant_message {
            session_state.push_assistant(content);
        }

        events.push(HarnessEvent::TurnCompleted {
            iterations: 1.min(self.max_iterations),
        });

        Ok(TurnOutcome {
            assistant_message,
            tool_calls: Vec::new(),
            events,
            session_state,
        })
    }
}

#[derive(Default)]
pub struct AgentHarnessBuilder {
    model_client: Option<Arc<dyn ModelClient>>,
    workspace: Option<Workspace>,
    tools: Option<ToolRegistry>,
    max_iterations: Option<u32>,
}

impl AgentHarnessBuilder {
    pub fn model_client<T>(mut self, model_client: T) -> Self
    where
        T: ModelClient + 'static,
    {
        self.model_client = Some(Arc::new(model_client));
        self
    }

    pub fn workspace(mut self, workspace: Workspace) -> Self {
        self.workspace = Some(workspace);
        self
    }

    pub fn tools(mut self, tools: ToolRegistry) -> Self {
        self.tools = Some(tools);
        self
    }

    pub fn max_iterations(mut self, max_iterations: u32) -> Self {
        self.max_iterations = Some(max_iterations);
        self
    }

    pub fn build(self) -> Result<AgentHarness, HarnessError> {
        Ok(AgentHarness {
            model_client: self
                .model_client
                .ok_or_else(|| HarnessError::Configuration {
                    message: "model client is required".to_string(),
                })?,
            workspace: self.workspace.ok_or_else(|| HarnessError::Configuration {
                message: "workspace is required".to_string(),
            })?,
            tools: self.tools.unwrap_or_default(),
            max_iterations: self.max_iterations.unwrap_or(4),
        })
    }
}
