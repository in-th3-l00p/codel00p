//! The application model: all TUI state in one place, mutated only by `update`.

use codel00p_harness::SessionState;
use codel00p_protocol::Viewer;
use tokio::sync::oneshot;

use crate::agent::{AgentRunOptions, McpServerSpec};
use crate::config::CliConfig;

use super::conversation::Conversation;
use super::overlay::{ModelChoice, Overlay};
use super::theme::Theme;

/// Live status of the in-flight turn, for the status bar and spinner.
#[derive(Clone, Debug, Default)]
pub(crate) struct TurnStatus {
    pub(crate) running: bool,
    pub(crate) current_tool: Option<String>,
    pub(crate) iterations: u32,
    pub(crate) finish_reason: Option<String>,
}

/// Read-only cloud context shown in the Org tab and status bar.
#[derive(Clone, Debug, Default)]
pub(crate) struct CloudState {
    pub(crate) viewer: Option<Viewer>,
    pub(crate) error: Option<String>,
    /// Whether stored credentials exist (so we know cloud calls can be attempted).
    pub(crate) configured: bool,
}

pub(crate) struct App {
    pub(crate) config: CliConfig,
    pub(crate) options: AgentRunOptions,
    pub(crate) mcp_servers: Vec<McpServerSpec>,
    pub(crate) session_state: SessionState,
    pub(crate) persisted_message_count: usize,
    pub(crate) conversation: Conversation,
    pub(crate) input: String,
    pub(crate) overlay: Overlay,
    pub(crate) pending_permission: Option<oneshot::Sender<codel00p_harness::PermissionDecision>>,
    pub(crate) turn: TurnStatus,
    pub(crate) cloud: CloudState,
    pub(crate) theme: Theme,
    pub(crate) should_quit: bool,
    pub(crate) tick: u64,
    /// A newer release version if one is already known (from the update cache),
    /// shown as a header chip. Read once at startup; never blocks.
    pub(crate) update_available: Option<String>,
}

impl App {
    pub(crate) fn new(
        config: CliConfig,
        options: AgentRunOptions,
        mcp_servers: Vec<McpServerSpec>,
        session_state: SessionState,
        persisted_message_count: usize,
        cloud_configured: bool,
    ) -> Self {
        Self {
            config,
            options,
            mcp_servers,
            session_state,
            persisted_message_count,
            conversation: Conversation::default(),
            input: String::new(),
            overlay: Overlay::None,
            pending_permission: None,
            turn: TurnStatus::default(),
            cloud: CloudState {
                configured: cloud_configured,
                ..CloudState::default()
            },
            theme: Theme::default(),
            should_quit: false,
            tick: 0,
            update_available: crate::update::cached_newer_version(),
        }
    }

    pub(crate) fn session_label(&self) -> String {
        self.session_state.session_id().as_str().to_string()
    }

    /// Builds the model picker choices: the active model first (marked current),
    /// then a curated catalog of well-known models. Selecting any sets the model
    /// for later turns; a free-text path is offered via the picker filter + Enter
    /// on a catalog row, mirroring the CLI's unchecked `/model <id>`.
    pub(crate) fn model_choices(&self) -> Vec<ModelChoice> {
        let mut choices = vec![ModelChoice {
            provider: self.options.provider.clone(),
            model: self.options.model.clone(),
            note: Some("current".to_string()),
        }];
        for (provider, model) in MODEL_CATALOG {
            if *provider == self.options.provider && *model == self.options.model {
                continue;
            }
            choices.push(ModelChoice {
                provider: (*provider).to_string(),
                model: (*model).to_string(),
                note: None,
            });
        }
        choices
    }
}

/// A small, hand-maintained catalog of common models per provider. Suggestions
/// only — any model id remains reachable by switching providers via config.
const MODEL_CATALOG: &[(&str, &str)] = &[
    ("anthropic", "claude-opus-4-8"),
    ("anthropic", "claude-sonnet-4-6"),
    ("anthropic", "claude-haiku-4-5"),
    ("openai", "gpt-4o"),
    ("openai", "gpt-4o-mini"),
    ("openai", "o3"),
    ("gemini", "gemini-2.0-flash"),
    ("gemini", "gemini-1.5-pro"),
    ("openrouter", "anthropic/claude-opus-4-8"),
    ("openrouter", "openai/gpt-4o"),
];
